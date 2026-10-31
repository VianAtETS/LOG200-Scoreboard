//! Classement Elo tenu à jour partie par partie, publié pour la page dans `summary.json`,
//! `teams/<équipe>.jsonl` et `timeline.jsonl` (format dans docs/formats.md).

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{self, BufWriter, ErrorKind, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::Serialize;

use isolation::board::{ByColor, Color};
use isolation::record::{GameRecord, Player, Reason, TournamentGame};

use crate::journal::append_jsonl;

pub const START: f64 = 1500.0;
const K_PROVISIONAL: f64 = 40.0;
const K_STABLE: f64 = 20.0;
/// Parties à K provisoire après chaque nouveau build.
const PROVISIONAL_GAMES: u32 = 20;
const RECENT_GAMES: usize = 50;
/// Clé de `scopes` pour toutes les sessions confondues.
const ALL: &str = "";

fn expected(ra: f64, rb: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((rb - ra) / 400.0))
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Temps par coup d'un build : les temps sont des ms entières, un compte par valeur suffit.
#[derive(Default)]
struct Pace {
    counts: BTreeMap<u64, u64>,
    total: u64,
    sum: u64,
    limit: u64,
}

impl Pace {
    /// Rang le plus proche.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rang positif, bien en deçà de 2^52"
    )]
    fn percentile(&self, p: f64) -> u64 {
        let rank = ((p * self.total as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (&ms, &n) in &self.counts {
            seen += n;
            if seen >= rank {
                return ms;
            }
        }
        0
    }

    fn view(&self) -> PaceView {
        PaceView {
            mean: (self.sum + self.total / 2) / self.total,
            p50: self.percentile(0.5),
            p95: self.percentile(0.95),
            p99: self.percentile(0.99),
            max: self.counts.keys().next_back().copied().unwrap_or(0),
            count: self.total,
            limit: self.limit,
        }
    }
}

struct Standing {
    /// Ordre d'arrivée : départage les ratings égaux.
    order: usize,
    rating: f64,
    games: u32,
    wins: u32,
    losses: u32,
    anchor: bool,
    build: Option<String>,
    games_on_build: u32,
    wins_on_build: u32,
    remote_on_build: bool,
    last: String,
    paces: HashMap<Option<String>, Pace>,
}

/// Une session, ou toutes.
#[derive(Default)]
struct Scope {
    teams: HashMap<String, Standing>,
    games: u64,
    time_ms: BTreeSet<u64>,
    recent: VecDeque<Row>,
    last: String,
}

impl Scope {
    fn enter(&mut self, player: &Player) -> &mut Standing {
        let order = self.teams.len();
        let s = self.teams.entry(player.team.clone()).or_insert_with(|| Standing {
            order,
            rating: START,
            games: 0,
            wins: 0,
            losses: 0,
            anchor: false,
            build: None,
            games_on_build: 0,
            wins_on_build: 0,
            remote_on_build: false,
            last: String::new(),
            paces: HashMap::new(),
        });
        s.anchor |= player.anchor;
        // Nouvelle soumission : le rating est conservé, K repasse en provisoire.
        if player.build.is_some() && player.build != s.build {
            s.build.clone_from(&player.build);
            s.games_on_build = 0;
            s.wins_on_build = 0;
            s.remote_on_build = false;
        }
        s.remote_on_build |= player.remote;
        s
    }

    /// Rating après la partie et variation, par couleur.
    fn play(&mut self, game: &GameRecord) -> ByColor<[f64; 2]> {
        let result = &game.result;
        self.games += 1;
        self.time_ms.insert(result.time_ms);
        self.last.clone_from(&game.ts);
        let players = ByColor {
            red: &game.red,
            black: &game.black,
        };
        let expected_red = expected(self.enter(players.red).rating, self.enter(players.black).rating);
        let timed = result.move_ms.len() == result.moves.len();
        let mut out = ByColor::default();
        for color in [Color::Red, Color::Black] {
            let won = result.winner == color;
            let exp = if color == Color::Red {
                expected_red
            } else {
                1.0 - expected_red
            };
            let s = self.enter(players[color]);
            let k = if s.games_on_build < PROVISIONAL_GAMES {
                K_PROVISIONAL
            } else {
                K_STABLE
            };
            let delta = if s.anchor {
                0.0
            } else {
                k * (f64::from(u8::from(won)) - exp)
            };
            s.rating += delta;
            s.games += 1;
            s.games_on_build += 1;
            if won {
                s.wins += 1;
                s.wins_on_build += 1;
            } else {
                s.losses += 1;
            }
            s.last.clone_from(&game.ts);
            // Le rouge joue les coups pairs.
            let first = usize::from(color == Color::Black);
            let ms: Vec<u64> = if timed {
                result.move_ms.iter().skip(first).step_by(2).copied().collect()
            } else {
                vec![]
            };
            if !ms.is_empty() {
                let pace = s.paces.entry(players[color].build.clone()).or_insert_with(|| Pace {
                    limit: u64::MAX,
                    ..Pace::default()
                });
                for m in ms {
                    *pace.counts.entry(m).or_default() += 1;
                    pace.total += 1;
                    pace.sum += m;
                }
                pace.limit = pace.limit.min(result.time_ms);
            }
            out[color] = [round2(s.rating), round2(delta)];
        }
        out
    }

    fn push_recent(&mut self, row: Row) {
        if self.recent.len() == RECENT_GAMES {
            self.recent.pop_front();
        }
        self.recent.push_back(row);
    }

    /// Meilleur rating d'abord. Temps par coup : ceux du build courant de l'équipe.
    fn ratings(&self) -> Vec<RatingRow<'_>> {
        let mut rows: Vec<_> = self.teams.iter().collect();
        rows.sort_by(|(_, a), (_, b)| b.rating.total_cmp(&a.rating).then(a.order.cmp(&b.order)));
        rows.into_iter()
            .map(|(team, s)| RatingRow {
                team,
                rating: round2(s.rating),
                games: s.games,
                wins: s.wins,
                losses: s.losses,
                build_games: s.games_on_build,
                build_wins: s.wins_on_build,
                anchor: s.anchor,
                provisional: !s.anchor && s.games_on_build < PROVISIONAL_GAMES,
                remote: s.remote_on_build,
                build: s.build.as_deref(),
                last: &s.last,
                pace: s.paces.get(&s.build).map(Pace::view),
            })
            .collect()
    }

    fn view(&self) -> ScopeView<'_> {
        ScopeView {
            games: self.games,
            time_ms: &self.time_ms,
            ratings: self.ratings(),
            recent: &self.recent,
        }
    }
}

#[derive(Serialize, Clone)]
struct Side {
    team: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    remote: bool,
    /// Rating après la partie et variation, toutes sessions puis dans la session.
    all: [f64; 2],
    session: [f64; 2],
    /// Les mêmes sans les parties distantes, s'ils diffèrent. Absent d'une partie distante.
    #[serde(skip_serializing_if = "Option::is_none")]
    verified: Option<Verified>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
struct Verified {
    all: [f64; 2],
    session: [f64; 2],
}

/// Une partie telle que la page l'affiche, sans les coups : `at` donne ses octets dans `games.jsonl`.
#[derive(Serialize, Clone)]
struct Row {
    v: u32,
    id: String,
    ts: String,
    session: String,
    time_ms: u64,
    winner: Color,
    reason: Reason,
    moves: usize,
    duration_ms: u64,
    red: Side,
    black: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    tournament: Option<TournamentGame>,
    at: [u64; 2],
}

/// Ligne de `timeline.jsonl` : le strict nécessaire pour rejouer les ratings de toutes les équipes.
#[derive(Serialize)]
struct Tick<'a> {
    v: u32,
    id: &'a str,
    ts: &'a str,
    session: &'a str,
    red: &'a Side,
    black: &'a Side,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    tournament: bool,
}

impl Row {
    fn tick(&self) -> Tick<'_> {
        Tick {
            v: self.v,
            id: &self.id,
            ts: &self.ts,
            session: &self.session,
            red: &self.red,
            black: &self.black,
            tournament: self.tournament.is_some(),
        }
    }
}

#[derive(Serialize)]
struct PaceView {
    mean: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
    count: u64,
    limit: u64,
}

#[derive(Serialize)]
struct RatingRow<'a> {
    team: &'a str,
    rating: f64,
    games: u32,
    wins: u32,
    losses: u32,
    build_games: u32,
    build_wins: u32,
    anchor: bool,
    provisional: bool,
    remote: bool,
    build: Option<&'a str>,
    last: &'a str,
    pace: Option<PaceView>,
}

#[derive(Serialize)]
struct ScopeView<'a> {
    games: u64,
    time_ms: &'a BTreeSet<u64>,
    ratings: Vec<RatingRow<'a>>,
    recent: &'a VecDeque<Row>,
}

#[derive(Serialize)]
struct Summary<'a> {
    v: u32,
    sessions: Vec<&'a str>,
    builds: &'a BTreeMap<String, Vec<String>>,
    scopes: BTreeMap<&'a str, ScopeView<'a>>,
    /// Mêmes clés que `scopes`, sans les parties distantes ; absent tant qu'il n'y en a pas eu.
    #[serde(skip_serializing_if = "Option::is_none")]
    verified: Option<BTreeMap<&'a str, ScopeView<'a>>>,
}

/// Toutes les sessions, puis une par session.
#[derive(Default)]
struct Scopes {
    all: Scope,
    sessions: HashMap<String, Scope>,
}

impl Scopes {
    fn play(&mut self, game: &GameRecord) -> (ByColor<[f64; 2]>, ByColor<[f64; 2]>) {
        let all = if game.tournament.is_some() {
            let rating = |p: &Player| [round2(self.all.teams.get(&p.team).map_or(START, |s| s.rating)), 0.0];
            ByColor {
                red: rating(&game.red),
                black: rating(&game.black),
            }
        } else {
            self.all.play(game)
        };
        (all, self.session(&game.session).play(game))
    }

    fn session(&mut self, name: &str) -> &mut Scope {
        self.sessions.entry(name.to_string()).or_default()
    }

    fn push_recent(&mut self, row: &Row) {
        if row.tournament.is_none() {
            self.all.push_recent(row.clone());
        }
        self.session(&row.session).push_recent(row.clone());
    }

    fn views<'a>(&'a self, sessions: &[&'a str]) -> BTreeMap<&'a str, ScopeView<'a>> {
        let mut views: BTreeMap<_, _> = sessions
            .iter()
            .filter_map(|&name| self.sessions.get(name).map(|scope| (name, scope.view())))
            .collect();
        views.insert(ALL, self.all.view());
        views
    }
}

pub struct Standings {
    dir: PathBuf,
    full: Scopes,
    /// Sans les parties distantes. Chaque session de `full` y a une entrée, même vide.
    verified: Scopes,
    any_remote: bool,
    builds: BTreeMap<String, Vec<String>>,
}

impl Standings {
    fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            full: Scopes::default(),
            verified: Scopes::default(),
            any_remote: false,
            builds: BTreeMap::new(),
        }
    }

    /// Rejoue l'historique, dans l'ordre du fichier, et réécrit les fichiers d'équipe, la timeline et le résumé.
    pub fn from_history(dir: &Path, games: &[GameRecord], at: &[Range<u64>]) -> io::Result<Self> {
        let mut standings = Self::new(dir);
        let teams = dir.join("teams");
        match fs::remove_dir_all(&teams) {
            Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
            _ => fs::create_dir_all(&teams)?,
        }
        let mut files: HashMap<String, BufWriter<File>> = HashMap::new();
        let mut timeline = BufWriter::new(File::create(dir.join("timeline.jsonl"))?);
        for (game, at) in games.iter().zip(at) {
            let row = standings.play(game, at.clone());
            serde_json::to_writer(&mut timeline, &row.tick())?;
            timeline.write_all(b"\n")?;
            for team in [&game.red.team, &game.black.team] {
                let Some(path) = standings.team_path(team) else {
                    continue;
                };
                let file = match files.entry(team.clone()) {
                    Entry::Occupied(e) => e.into_mut(),
                    Entry::Vacant(e) => e.insert(BufWriter::new(File::create(path)?)),
                };
                serde_json::to_writer(&mut *file, &row)?;
                file.write_all(b"\n")?;
            }
        }
        for file in files.values_mut() {
            file.flush()?;
        }
        timeline.flush()?;
        standings.write_summary()?;
        Ok(standings)
    }

    /// Rating toutes sessions, `START` pour une équipe qui n'a pas encore joué.
    pub fn rating(&self, team: &str) -> f64 {
        self.full.all.teams.get(team).map_or(START, |s| s.rating)
    }

    /// Rating sans parties distantes et ordre d'arrivée dans `session`.
    pub fn verified_session(&self, session: &str, team: &str) -> Option<(f64, usize)> {
        let s = self.verified.sessions.get(session)?.teams.get(team)?;
        Some((s.rating, s.order))
    }

    /// `at` : octets de la partie dans `games.jsonl`.
    pub fn record(&mut self, game: &GameRecord, at: Range<u64>) -> io::Result<()> {
        let row = self.play(game, at);
        for team in [&game.red.team, &game.black.team] {
            if let Some(path) = self.team_path(team) {
                append_jsonl(&path, &row)?;
            }
        }
        append_jsonl(&self.dir.join("timeline.jsonl"), &row.tick())?;
        self.write_summary()
    }

    fn play(&mut self, game: &GameRecord, at: Range<u64>) -> Row {
        for player in [&game.red, &game.black] {
            if let Some(build) = &player.build {
                let list = self.builds.entry(player.team.clone()).or_default();
                if !list.contains(build) {
                    list.push(build.clone());
                }
            }
        }
        let (all, in_session) = self.full.play(game);
        let remote = game.red.remote || game.black.remote;
        self.any_remote |= remote;
        self.verified.session(&game.session);
        let verified = (!remote).then(|| self.verified.play(game));
        let side = |player: &Player, color: Color| Side {
            team: player.team.clone(),
            build: player.build.clone(),
            remote: player.remote,
            all: all[color],
            session: in_session[color],
            verified: verified
                .map(|(v_all, v_session)| Verified {
                    all: v_all[color],
                    session: v_session[color],
                })
                .filter(|v| {
                    *v != Verified {
                        all: all[color],
                        session: in_session[color],
                    }
                }),
        };
        let result = &game.result;
        let row = Row {
            v: 1,
            id: game.id.clone(),
            ts: game.ts.clone(),
            session: game.session.clone(),
            time_ms: result.time_ms,
            winner: result.winner,
            reason: result.reason,
            moves: result.moves.len(),
            duration_ms: result.duration_ms,
            red: side(&game.red, Color::Red),
            black: side(&game.black, Color::Black),
            tournament: game.tournament.clone(),
            at: [at.start, at.end - at.start],
        };
        self.full.push_recent(&row);
        if !remote {
            self.verified.push_recent(&row);
        }
        row
    }

    /// `None` si le nom ne fait pas un nom de fichier sûr : les équipes de `ladder.toml` sont libres.
    fn team_path(&self, team: &str) -> Option<PathBuf> {
        let safe =
            (1..=64).contains(&team.len()) && team.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        safe.then(|| self.dir.join("teams").join(format!("{team}.jsonl")))
    }

    fn summary(&self) -> Summary<'_> {
        let mut sessions: Vec<_> = self.full.sessions.iter().collect();
        sessions.sort_by(|(_, a), (_, b)| b.last.cmp(&a.last));
        let sessions: Vec<&str> = sessions.into_iter().map(|(name, _)| name.as_str()).collect();
        Summary {
            v: 1,
            scopes: self.full.views(&sessions),
            verified: self.any_remote.then(|| self.verified.views(&sessions)),
            sessions,
            builds: &self.builds,
        }
    }

    /// Écrit à côté puis renomme : un lecteur ne voit jamais un résumé à moitié écrit.
    fn write_summary(&self) -> io::Result<()> {
        let tmp = self.dir.join("summary.json.tmp");
        fs::write(&tmp, serde_json::to_vec(&self.summary())?)?;
        fs::rename(tmp, self.dir.join("summary.json"))
    }
}

#[cfg(test)]
mod tests {
    use isolation::record::{GameResult, InvalidCounts, RECORD_VERSION};

    use super::*;

    fn player(team: &str) -> Player {
        Player {
            team: team.into(),
            build: None,
            anchor: false,
            remote: false,
        }
    }

    fn game(red: &str, black: &str, winner: Color) -> GameRecord {
        GameRecord {
            id: format!("20261024T120000.000Z-{red}-{black}"),
            session: "s1".into(),
            ts: "2026-10-24T12:00:00.000Z".into(),
            red: player(red),
            black: player(black),
            tournament: None,
            result: GameResult {
                v: RECORD_VERSION,
                time_ms: 500,
                opening: vec![],
                moves: vec![],
                winner,
                reason: Reason::NoMoves,
                invalid: InvalidCounts::default(),
                duration_ms: 0,
                move_ms: vec![],
            },
        }
    }

    fn rating<'a>(ratings: &'a [RatingRow<'a>], team: &str) -> &'a RatingRow<'a> {
        ratings.iter().find(|r| r.team == team).unwrap()
    }

    #[test]
    fn expected_score_is_symmetric() {
        assert!((expected(1500.0, 1500.0) - 0.5).abs() < 1e-12);
        assert!((expected(1600.0, 1400.0) + expected(1400.0, 1600.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn first_game_moves_both_ratings_by_half_k() {
        let mut scope = Scope::default();
        let out = scope.play(&game("a", "b", Color::Red));
        assert_eq!(out.red, [START + 20.0, 20.0]);
        assert_eq!(out.black, [START - 20.0, -20.0]);
        let ratings = scope.ratings();
        assert_eq!(ratings[0].team, "a");
        assert_eq!((ratings[0].wins, ratings[1].losses), (1, 1));
        assert!(ratings[0].provisional);
    }

    #[test]
    fn anchor_rating_never_moves() {
        let mut scope = Scope::default();
        let mut g = game("a", "ref", Color::Black);
        g.black.anchor = true;
        assert_eq!(scope.play(&g).black, [START, 0.0]);
        assert_eq!(scope.play(&game("ref", "a", Color::Red)).red, [START, 0.0]);
        let ratings = scope.ratings();
        let r = rating(&ratings, "ref");
        assert_eq!((r.rating, r.games, r.provisional, r.anchor), (START, 2, false, true));
    }

    #[test]
    fn k_drops_after_the_provisional_period_and_resets_on_a_new_build() {
        let mut scope = Scope::default();
        for i in 0..PROVISIONAL_GAMES {
            let mut g = game("a", "b", if i % 2 == 0 { Color::Black } else { Color::Red });
            g.red.build = Some("v1".into());
            scope.play(&g);
        }
        assert!(!rating(&scope.ratings(), "a").provisional);
        let mut next = game("a", "b", Color::Red);
        next.red.build = Some("v2".into());
        scope.play(&next);
        let ratings = scope.ratings();
        let a = rating(&ratings, "a");
        assert!(a.provisional);
        assert_eq!((a.games, a.build), (PROVISIONAL_GAMES + 1, Some("v2")));
        assert_eq!((a.build_games, a.build_wins), (1, 1));
    }

    #[test]
    fn remote_follows_the_current_build_and_the_game_rows() {
        let mut standings = Standings::new(Path::new("d"));
        let mut g = game("a", "b", Color::Red);
        g.red.build = Some("v1".into());
        g.red.remote = true;
        let row = serde_json::to_value(standings.play(&g, 0..1)).unwrap();
        assert_eq!(row["red"]["remote"], true);
        assert!(row["black"].get("remote").is_none());
        g.red.remote = false;
        standings.play(&g, 1..2);
        let ratings = standings.full.all.ratings();
        assert!(rating(&ratings, "a").remote);
        assert!(!rating(&ratings, "b").remote);

        g.red.build = Some("v2".into());
        standings.play(&g, 2..3);
        assert!(!rating(&standings.full.all.ratings(), "a").remote);
    }

    #[test]
    fn verified_ranking_leaves_out_remote_games() {
        let mut standings = Standings::new(Path::new("d"));
        let first = serde_json::to_value(standings.play(&game("a", "b", Color::Red), 0..1)).unwrap();
        assert!(first["red"].get("verified").is_none());
        assert!(standings.summary().verified.is_none());

        let mut remote = game("c", "a", Color::Black);
        remote.red.remote = true;
        let row = serde_json::to_value(standings.play(&remote, 1..2)).unwrap();
        assert!(row["black"].get("verified").is_none());
        // a a gagné contre l'équipe distante : seul le classement complet le compte.
        let after = serde_json::to_value(standings.play(&game("a", "b", Color::Red), 2..3)).unwrap();
        let (full, verified) = (&after["red"]["all"][0], &after["red"]["verified"]["all"][0]);
        assert!(full.as_f64().unwrap() > verified.as_f64().unwrap());

        let mut other = game("x", "y", Color::Red);
        other.session = "s2".into();
        let untouched = serde_json::to_value(standings.play(&other, 3..4)).unwrap();
        assert!(untouched["red"].get("verified").is_none());

        let summary = serde_json::to_value(standings.summary()).unwrap();
        let verified = &summary["verified"];
        assert_eq!(verified[""]["games"], 3);
        assert_eq!(summary["scopes"][""]["games"], 4);
        assert_eq!(verified["s1"]["recent"].as_array().unwrap().len(), 2);
        assert!(
            verified["s1"]["ratings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["team"] != "c")
        );
        assert_eq!(verified["s2"]["games"], 1);
    }

    #[test]
    fn tournament_games_count_only_in_their_session() {
        let mut standings = Standings::new(Path::new("d"));
        standings.play(&game("a", "b", Color::Red), 0..1);
        let before = (standings.rating("a"), standings.rating("b"));
        let mut t = game("b", "a", Color::Red);
        t.session = "tournoi-2026-11-08".into();
        t.tournament = Some(TournamentGame {
            id: "tournoi-2026-11-08".into(),
            match_id: "W1".into(),
            game: 1,
        });
        let row = standings.play(&t, 1..2);
        assert_eq!((standings.rating("a"), standings.rating("b")), before);
        assert_eq!(row.black.all, [before.0, 0.0]);
        assert!(row.black.session[1] < 0.0);
        assert_eq!(serde_json::to_value(&row).unwrap()["tournament"]["match"], "W1");
        assert_eq!(serde_json::to_value(row.tick()).unwrap()["tournament"], true);
        let summary = serde_json::to_value(standings.summary()).unwrap();
        assert_eq!(summary["scopes"][""]["games"], 1);
        assert_eq!(summary["scopes"][""]["recent"].as_array().unwrap().len(), 1);
        assert_eq!(summary["scopes"]["tournoi-2026-11-08"]["games"], 1);
        let ladder = standings.play(&game("a", "b", Color::Red), 2..3);
        assert!(serde_json::to_value(ladder.tick()).unwrap().get("tournament").is_none());
    }

    #[test]
    fn verified_session_ignores_remote_games_and_other_sessions() {
        let mut standings = Standings::new(Path::new("d"));
        standings.play(&game("a", "b", Color::Red), 0..1);
        let mut remote = game("b", "a", Color::Red);
        remote.red.remote = true;
        standings.play(&remote, 0..1);
        let mut elsewhere = game("b", "a", Color::Red);
        elsewhere.session = "s2".into();
        standings.play(&elsewhere, 0..1);
        let (a, a_order) = standings.verified_session("s1", "a").unwrap();
        let (b, b_order) = standings.verified_session("s1", "b").unwrap();
        assert!(a > b, "{a} {b}");
        assert_eq!((a_order, b_order), (0, 1));
        assert!(standings.verified_session("s1", "c").is_none());
    }

    #[test]
    fn a_session_of_remote_games_only_still_has_a_verified_entry() {
        let mut standings = Standings::new(Path::new("d"));
        let mut remote = game("c", "a", Color::Black);
        remote.red.remote = true;
        standings.play(&remote, 0..1);
        let summary = serde_json::to_value(standings.summary()).unwrap();
        assert_eq!(summary["verified"]["s1"]["games"], 0);
        assert_eq!(summary["verified"][""]["ratings"], serde_json::json!([]));
    }

    #[test]
    fn pace_uses_nearest_rank_on_the_current_build_and_skips_inconsistent_games() {
        let mut scope = Scope::default();
        let mut g = game("a", "b", Color::Red);
        g.red.build = Some("v1".into());
        g.result.moves = vec![String::new(); 200];
        g.result.move_ms = (1..=100).flat_map(|i| [i, 1000]).collect();
        scope.play(&g);
        let mut broken = game("a", "b", Color::Red);
        broken.red.build = Some("v1".into());
        broken.result.moves = vec![String::new(); 2];
        broken.result.move_ms = vec![9999];
        scope.play(&broken);
        let ratings = scope.ratings();
        let pace = rating(&ratings, "a").pace.as_ref().unwrap();
        assert_eq!((pace.p50, pace.p95, pace.p99, pace.max), (50, 95, 99, 100));
        assert_eq!((pace.count, pace.limit, pace.mean), (100, 500, 51));
        assert!(rating(&ratings, "b").pace.is_some());

        let mut newer = game("a", "b", Color::Red);
        newer.red.build = Some("v2".into());
        scope.play(&newer);
        assert!(rating(&scope.ratings(), "a").pace.is_none());
    }

    #[test]
    fn files_follow_sessions_builds_and_byte_offsets() {
        let dir = std::env::temp_dir().join(format!("isolation-standings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("teams")).unwrap();
        fs::write(dir.join("teams/stale.jsonl"), "").unwrap();

        let mut g1 = game("a", "b", Color::Red);
        g1.red.build = Some("h1".into());
        let mut standings =
            Standings::from_history(&dir, std::slice::from_ref(&g1), std::slice::from_ref(&(0..10))).unwrap();
        assert!(!dir.join("teams/stale.jsonl").exists());

        let mut g2 = game("b", "a", Color::Red);
        g2.session = "s2".into();
        g2.ts = "2026-10-25T12:00:00.000Z".into();
        g2.black.build = Some("h2".into());
        standings.record(&g2, 10..25).unwrap();
        assert!((standings.rating("a") - (1520.0 - 22.29)).abs() < 0.01);
        assert_eq!(standings.rating("c"), START);

        let lines = fs::read_to_string(dir.join("teams/a.jsonl")).unwrap();
        let rows: Vec<serde_json::Value> = lines.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["at"], serde_json::json!([10, 15]));
        // Toutes sessions : a part de 1520 contre 1480. Dans s2, les deux repartent de 1500.
        assert_eq!(rows[1]["black"]["all"][1], serde_json::json!(-22.29));
        assert_eq!(rows[1]["black"]["session"], serde_json::json!([1480.0, -20.0]));

        let summary: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("summary.json")).unwrap()).unwrap();
        assert_eq!(summary["sessions"], serde_json::json!(["s2", "s1"]));
        assert_eq!(summary["builds"]["a"], serde_json::json!(["h1", "h2"]));
        assert_eq!(summary["scopes"][""]["games"], 2);
        assert_eq!(summary["scopes"]["s2"]["recent"].as_array().unwrap().len(), 1);
        assert_eq!(summary["scopes"]["s1"]["ratings"][0]["team"], "a");
        assert!(!dir.join("summary.json.tmp").exists());

        let timeline = fs::read_to_string(dir.join("timeline.jsonl")).unwrap();
        let ticks: Vec<serde_json::Value> = timeline.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(ticks.len(), 2);
        assert_eq!(ticks[1]["session"], "s2");
        assert_eq!(ticks[1]["black"], rows[1]["black"]);
        assert!(ticks[1].get("at").is_none());
        Standings::from_history(&dir, &[g1, g2], &[0..10, 10..25]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("timeline.jsonl")).unwrap(), timeline);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsafe_team_names_get_no_file() {
        let s = Standings::new(Path::new("d"));
        assert!(s.team_path("eq-07_b").is_some());
        for bad in ["", "../x", "a/b", "a.b", &"x".repeat(65)] {
            assert!(s.team_path(bad).is_none(), "{bad}");
        }
    }
}
