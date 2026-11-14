//! Base SQLite du ladder, source de vérité des parties et des soumissions. Les requêtes sont dans
//! `sql/`, le schéma dans `sql/schema.sql` (décrit dans docs/formats.md).

use std::fs;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, named_params};

use isolation_core::board::Color;
use isolation_core::record::{GameRecord, Jsonl};

const SCHEMA_VERSION: i64 = 1;

mod sql {
    pub const SCHEMA: &str = include_str!("sql/schema.sql");
    pub const IS_EMPTY: &str = include_str!("sql/is_empty.sql");
    pub const GAMES: &str = include_str!("sql/games.sql");
    pub const GAME: &str = include_str!("sql/game.sql");
    pub const GAMES_FILTER: &str = include_str!("sql/games_filter.sql");
    pub const INSERT_GAME: &str = include_str!("sql/insert_game.sql");
    pub const INSERT_MOVE: &str = include_str!("sql/insert_move.sql");
    pub const INSERT_RATING: &str = include_str!("sql/insert_rating.sql");
    pub const CLEAR_RATINGS: &str = include_str!("sql/clear_ratings.sql");
    pub const INSERT_SUBMISSION: &str = include_str!("sql/insert_submission.sql");
    pub const HAS_SUBMISSIONS: &str = include_str!("sql/has_submissions.sql");
}

/// Statistiques de `/api/stats`, chacune une requête qui rend directement son JSON.
#[derive(Clone, Copy)]
pub enum Stat {
    Teams,
    HeadToHead,
    Openings,
    Builds,
    Ratings,
    MoveTimes,
    Submissions,
}

impl Stat {
    pub const fn sql(self) -> &'static str {
        match self {
            Self::Teams => include_str!("sql/stats_teams.sql"),
            Self::HeadToHead => include_str!("sql/stats_head_to_head.sql"),
            Self::Openings => include_str!("sql/stats_openings.sql"),
            Self::Builds => include_str!("sql/stats_builds.sql"),
            Self::Ratings => include_str!("sql/stats_ratings.sql"),
            Self::MoveTimes => include_str!("sql/stats_move_times.sql"),
            Self::Submissions => include_str!("sql/stats_submissions.sql"),
        }
    }
}

pub struct Db {
    conn: Connection,
}

/// Filtres de `/api/games`, chacun facultatif.
#[derive(serde::Deserialize, utoipa::IntoParams, Default, PartialEq, Eq)]
#[into_params(parameter_in = Query)]
pub struct GameFilter {
    /// Les parties de cette équipe, en rouge ou en noir (`eq100`).
    pub team: Option<String>,
    /// Les parties contre cette équipe. Avec `team`, leurs face-à-face.
    pub opponent: Option<String>,
    /// Une session ou un tournoi (`automne-2026`), tournois compris.
    pub session: Option<String>,
    /// L'id d'une partie : seules celles jouées avant elle, pour remonter page par page en passant
    /// l'id de la première ligne reçue. Un id inconnu donne une réponse vide.
    pub before: Option<String>,
}

/// Rating d'une équipe après une partie : `[rating, variation]`, toutes sessions puis dans la session.
pub struct Rating {
    pub game: String,
    pub team: String,
    pub all: [f64; 2],
    pub session: [f64; 2],
}

/// Une soumission de jar et son verdict (`accepted`, `rejected` ou `error`), comme l'ancien
/// `submissions.jsonl`.
#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct Submission {
    pub ts: String,
    pub team: String,
    pub hash: String,
    pub verdict: String,
    pub detail: String,
}

fn name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn to_json<T: serde::Serialize>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(sql::SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { conn })
    }

    /// Connexion en lecture seule, pour l'API.
    pub fn open_read(path: &Path) -> rusqlite::Result<Self> {
        let conn =
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn is_empty(&self) -> rusqlite::Result<bool> {
        self.conn.query_row(sql::IS_EMPTY, [], |r| r.get(0))
    }

    pub fn insert_game(&self, game: &GameRecord) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        insert(&tx, game)?;
        tx.commit()
    }

    /// Toutes les parties, dans l'ordre où elles ont été jouées.
    pub fn games(&self) -> rusqlite::Result<Vec<GameRecord>> {
        let mut stmt = self.conn.prepare(sql::GAMES)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|text| {
            serde_json::from_str(&text?)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
        })
        .collect()
    }

    /// La ligne JSON d'une partie, telle qu'enregistrée.
    pub fn game_json(&self, id: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self.conn.prepare_cached(sql::GAME)?;
        let mut rows = stmt.query(named_params! { ":id": id })?;
        rows.next()?.map(|r| r.get(0)).transpose()
    }

    /// Les `limit` dernières parties qui passent `filter`, en JSONL, de la plus ancienne à la plus récente.
    pub fn games_jsonl(&self, filter: &GameFilter, limit: usize) -> rusqlite::Result<String> {
        let mut stmt = self.conn.prepare_cached(sql::GAMES_FILTER)?;
        let mut rows = stmt.query(named_params! {
            ":team": filter.team,
            ":opponent": filter.opponent,
            ":session": filter.session,
            ":before": filter.before,
            ":limit": limit,
        })?;
        let mut out = String::new();
        while let Some(row) = rows.next()? {
            out.push_str(row.get_ref(0)?.as_str()?);
            out.push('\n');
        }
        Ok(out)
    }

    /// `session` absente : toutes les sessions hors tournois. `team` ne sert qu'aux statistiques d'une équipe.
    pub fn stats(&self, which: Stat, session: Option<&str>, team: Option<&str>) -> rusqlite::Result<String> {
        let mut stmt = self.conn.prepare_cached(which.sql())?;
        // SQLite refuse un paramètre nommé absent de la requête.
        for (name, value) in [(":session", session), (":team", team)] {
            if let Some(index) = stmt.parameter_index(name)? {
                stmt.raw_bind_parameter(index, value)?;
            }
        }
        let mut rows = stmt.raw_query();
        rows.next()?.map_or(Ok("[]".into()), |r| r.get(0))
    }

    /// Migration depuis l'ancien `games.jsonl` : tout en une transaction, lignes illisibles ignorées.
    pub fn import_jsonl(&self, path: &Path) -> anyhow::Result<usize> {
        let jsonl = Jsonl::parse(&fs::read_to_string(path)?);
        for skipped in &jsonl.skipped {
            eprintln!("{} ligne {} ignorée : {}", path.display(), skipped.line, skipped.reason);
        }
        let tx = self.conn.unchecked_transaction()?;
        for game in &jsonl.games {
            insert(&tx, game)?;
        }
        tx.commit()?;
        Ok(jsonl.games.len())
    }

    /// Remplace toute la table : les ratings se recalculent au démarrage.
    pub fn replace_ratings(&self, ratings: impl IntoIterator<Item = Rating>) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(sql::CLEAR_RATINGS, [])?;
        for rating in ratings {
            insert_rating(&tx, &rating)?;
        }
        tx.commit()
    }

    pub fn insert_submission(&self, s: &Submission) -> rusqlite::Result<()> {
        self.conn
            .prepare_cached(sql::INSERT_SUBMISSION)?
            .execute(named_params! {
                ":ts": s.ts,
                ":team": s.team,
                ":hash": s.hash,
                ":verdict": s.verdict,
                ":detail": s.detail,
            })?;
        Ok(())
    }

    pub fn has_submissions(&self) -> rusqlite::Result<bool> {
        self.conn.query_row(sql::HAS_SUBMISSIONS, [], |r| r.get(0))
    }

    /// Migration depuis l'ancien `submissions.jsonl`, en une transaction. Lignes illisibles ignorées.
    pub fn import_submissions(&self, path: &Path) -> anyhow::Result<usize> {
        let text = fs::read_to_string(path)?;
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0;
        for (index, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            match serde_json::from_str::<Submission>(line) {
                Ok(row) => {
                    self.insert_submission(&row)?;
                    count += 1;
                }
                Err(e) => eprintln!("{} ligne {} ignorée : {e}", path.display(), index + 1),
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn add_ratings(&self, ratings: impl IntoIterator<Item = Rating>) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for rating in ratings {
            insert_rating(&tx, &rating)?;
        }
        tx.commit()
    }
}

fn insert(conn: &Connection, game: &GameRecord) -> rusqlite::Result<()> {
    let result = &game.result;
    let tournament = game.tournament.as_ref();
    conn.prepare_cached(sql::INSERT_GAME)?.execute(named_params! {
        ":id": game.id,
        ":session": game.session,
        ":ts": game.ts,
        ":time_ms": result.time_ms,
        ":red_team": game.red.team,
        ":red_build": game.red.build,
        ":red_remote": game.red.remote,
        ":black_team": game.black.team,
        ":black_build": game.black.build,
        ":black_remote": game.black.remote,
        ":winner": name(&result.winner),
        ":reason": name(&result.reason),
        ":invalid_red": result.invalid.red,
        ":invalid_black": result.invalid.black,
        ":duration_ms": result.duration_ms,
        ":plies": result.moves.len(),
        ":opening": to_json(&result.opening)?,
        ":tournament_id": tournament.map(|t| &t.id),
        ":tournament_match": tournament.map(|t| &t.match_id),
        ":tournament_game": tournament.map(|t| t.game),
        ":record": to_json(game)?,
    })?;
    let seq = conn.last_insert_rowid();
    let mut stmt = conn.prepare_cached(sql::INSERT_MOVE)?;
    // L'ouverture a un nombre pair de demi-coups : le rouge joue le premier coup client.
    for (ply, mv) in result.moves.iter().enumerate() {
        let color = if ply % 2 == 0 { Color::Red } else { Color::Black };
        stmt.execute(named_params! {
            ":game": seq,
            ":ply": ply,
            ":color": name(&color),
            ":move": mv,
            ":ms": result.move_ms.get(ply),
        })?;
    }
    Ok(())
}

fn insert_rating(conn: &Connection, r: &Rating) -> rusqlite::Result<()> {
    conn.prepare_cached(sql::INSERT_RATING)?.execute(named_params! {
        ":game_id": r.game,
        ":team": r.team,
        ":all_rating": r.all[0],
        ":all_delta": r.all[1],
        ":session_rating": r.session[0],
        ":session_delta": r.session[1],
    })?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use isolation_core::record::{GameResult, InvalidCounts, Player, RECORD_VERSION, Reason, TournamentGame};

    pub(crate) fn game(id: &str, red: &str, black: &str, winner: Color) -> GameRecord {
        let player = |team: &str| Player {
            team: team.into(),
            build: None,
            anchor: false,
            remote: false,
        };
        GameRecord {
            id: id.into(),
            session: "s1".into(),
            ts: "2026-10-24T12:00:00.000Z".into(),
            red: player(red),
            black: player(black),
            tournament: None,
            node: None,
            result: GameResult {
                v: RECORD_VERSION,
                time_ms: 500,
                opening: vec!["G1-I1-C12".into(), "F12-D10-F1".into()],
                moves: vec!["I1-F4-A7".into(), "D10-C10-I7".into(), "F4-F6-F10".into()],
                winner,
                reason: Reason::NoMoves,
                invalid: InvalidCounts { red: 0, black: 1 },
                duration_ms: 1500,
                move_ms: vec![10, 20, 30],
            },
        }
    }

    pub(crate) fn memory() -> Db {
        Db::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn games_come_back_identical_and_in_order() {
        let db = memory();
        assert!(db.is_empty().unwrap());
        let mut first = game("g2", "a", "b", Color::Red);
        first.red.build = Some("h1".into());
        first.black.remote = true;
        first.tournament = Some(TournamentGame {
            id: "t".into(),
            match_id: "W1".into(),
            game: 2,
        });
        let second = game("g1", "b", "a", Color::Black);
        db.insert_game(&first).unwrap();
        db.insert_game(&second).unwrap();
        assert!(!db.is_empty().unwrap());
        assert_eq!(db.games().unwrap(), vec![first.clone(), second]);
        let json: GameRecord = serde_json::from_str(&db.game_json("g2").unwrap().unwrap()).unwrap();
        assert_eq!(json, first);
        assert_eq!(db.game_json("absent").unwrap(), None);
    }

    #[test]
    fn recent_games_are_the_last_ones_in_order() {
        let db = memory();
        assert_eq!(db.games_jsonl(&GameFilter::default(), 2).unwrap(), "");
        for id in ["g1", "g2", "g3"] {
            db.insert_game(&game(id, "a", "b", Color::Red)).unwrap();
        }
        let ids: Vec<String> = db
            .games_jsonl(&GameFilter::default(), 2)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<GameRecord>(line).unwrap().id)
            .collect();
        assert_eq!(ids, ["g2", "g3"]);
    }

    #[test]
    fn columns_and_moves_are_queryable() {
        let db = memory();
        db.insert_game(&game("g1", "a", "b", Color::Black)).unwrap();
        let (winner, reason, plies, invalid): (String, String, i64, i64) = db
            .conn()
            .query_row(
                "SELECT winner, reason, plies, invalid_black FROM games WHERE id = 'g1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            (winner.as_str(), reason.as_str(), plies, invalid),
            ("black", "no_moves", 3, 1)
        );
        let moves: Vec<(String, String, i64)> = db
            .conn()
            .prepare("SELECT color, move, ms FROM moves JOIN games ON seq = game WHERE id = 'g1' ORDER BY ply")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(moves[0], ("red".into(), "I1-F4-A7".into(), 10));
        assert_eq!(moves[1].0, "black");
        assert_eq!(moves.len(), 3);
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let db = memory();
        db.insert_game(&game("g1", "a", "b", Color::Red)).unwrap();
        assert!(db.insert_game(&game("g1", "a", "b", Color::Red)).is_err());
        assert_eq!(db.games().unwrap().len(), 1);
    }

    #[test]
    fn import_reads_a_jsonl_and_skips_broken_lines() {
        let path = std::env::temp_dir().join(format!("isolation-db-import-{}.jsonl", std::process::id()));
        let games = [game("g1", "a", "b", Color::Red), game("g2", "b", "a", Color::Red)];
        let mut text: String = games.iter().map(|g| serde_json::to_string(g).unwrap() + "\n").collect();
        text.push_str("{\"id\": \"cut");
        fs::write(&path, text).unwrap();
        let db = memory();
        assert_eq!(db.import_jsonl(&path).unwrap(), 2);
        assert_eq!(db.games().unwrap(), games);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn ratings_are_replaced_then_appended() {
        let db = memory();
        let rating = |game: &str, team: &str| Rating {
            game: game.into(),
            team: team.into(),
            all: [1520.0, 20.0],
            session: [1510.0, 10.0],
        };
        db.replace_ratings([rating("g1", "a"), rating("g1", "b")]).unwrap();
        db.replace_ratings([rating("g1", "a")]).unwrap();
        db.add_ratings([rating("g2", "a")]).unwrap();
        let count: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM ratings WHERE team = 'a'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    /// a bat b deux fois (en rouge puis en noir, au temps), bat c dans s2 avec un nouveau build,
    /// et perd contre b en tournoi.
    fn league() -> Db {
        let db = memory();
        let mut g1 = game("g1", "a", "b", Color::Red);
        g1.red.build = Some("h1".into());
        g1.ts = "2026-10-24T12:00:00.000Z".into();
        g1.result.move_ms = vec![600, 20, 480];
        let mut g2 = game("g2", "b", "a", Color::Black);
        g2.black.build = Some("h1".into());
        g2.result.reason = Reason::Timeout;
        g2.result.opening = vec!["A1-B2-C3".into(), "D4-E5-F6".into()];
        g2.result.moves = vec!["F4-F6-F10".into(), "I1-F4-A7".into()];
        g2.ts = "2026-10-24T13:00:00.000Z".into();
        let mut g3 = game("g3", "a", "c", Color::Red);
        g3.red.build = Some("h2".into());
        g3.session = "s2".into();
        g3.ts = "2026-10-25T12:00:00.000Z".into();
        let mut g4 = game("g4", "a", "b", Color::Black);
        g4.session = "t1".into();
        g4.tournament = Some(TournamentGame {
            id: "t1".into(),
            match_id: "W1".into(),
            game: 1,
        });
        for g in [g1, g2, g3, g4] {
            db.insert_game(&g).unwrap();
        }
        db
    }

    fn stat(db: &Db, stat: Stat, session: Option<&str>, team: Option<&str>) -> serde_json::Value {
        let json = db.stats(stat, session, team).unwrap();
        crate::openapi::check(stat, &json);
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn team_records_leave_tournaments_out_of_the_all_sessions_scope() {
        let db = league();
        let teams = stat(&db, Stat::Teams, None, None);
        assert_eq!(
            teams,
            serde_json::json!([
                {"team": "a", "games": 3, "wins": 3, "red": {"games": 2, "wins": 2},
                 "black": {"games": 1, "wins": 1}, "losses": {}},
                {"team": "b", "games": 2, "wins": 0, "red": {"games": 1, "wins": 0},
                 "black": {"games": 1, "wins": 0}, "losses": {"no_moves": 1, "timeout": 1}},
                {"team": "c", "games": 1, "wins": 0, "red": {"games": 0, "wins": 0},
                 "black": {"games": 1, "wins": 0}, "losses": {"no_moves": 1}},
            ])
        );
        let tournament = stat(&db, Stat::Teams, Some("t1"), None);
        assert_eq!(tournament[0]["team"], "b");
        assert_eq!(tournament[0]["wins"], 1);
        assert_eq!(stat(&db, Stat::Teams, Some("s1"), None).as_array().unwrap().len(), 2);
        assert_eq!(stat(&db, Stat::Teams, Some("absente"), None), serde_json::json!([]));
    }

    #[test]
    fn head_to_head_counts_each_pair_once() {
        let db = league();
        assert_eq!(
            stat(&db, Stat::HeadToHead, None, None),
            serde_json::json!([
                {"a": "a", "b": "b", "games": 2, "a_wins": 2, "b_wins": 0},
                {"a": "a", "b": "c", "games": 1, "a_wins": 1, "b_wins": 0},
            ])
        );
    }

    #[test]
    fn openings_and_first_moves_carry_their_win_rates() {
        let db = league();
        let openings = stat(&db, Stat::Openings, None, None);
        assert_eq!(
            openings["openings"],
            serde_json::json!([
                {"opening": ["G1-I1-C12", "F12-D10-F1"], "games": 2, "red_wins": 2},
                {"opening": ["A1-B2-C3", "D4-E5-F6"], "games": 1, "red_wins": 0},
            ])
        );
        assert_eq!(
            openings["first_moves"]["red"],
            serde_json::json!([
                {"move": "I1-F4-A7", "games": 2, "wins": 2},
                {"move": "F4-F6-F10", "games": 1, "wins": 0},
            ])
        );
        assert_eq!(openings["first_moves"]["black"][0]["move"], "D10-C10-I7");
        assert_eq!(
            openings["first_moves"]["black"][1],
            serde_json::json!({"move": "I1-F4-A7", "games": 1, "wins": 1})
        );
    }

    #[test]
    fn builds_follow_the_order_they_first_played() {
        let db = league();
        assert_eq!(
            stat(&db, Stat::Builds, None, Some("a")),
            serde_json::json!([
                {"build": "h1", "games": 2, "wins": 2, "first": "2026-10-24T12:00:00.000Z", "last": "2026-10-24T13:00:00.000Z"},
                {"build": "h2", "games": 1, "wins": 1, "first": "2026-10-25T12:00:00.000Z", "last": "2026-10-25T12:00:00.000Z"},
            ])
        );
        assert_eq!(
            stat(&db, Stat::Builds, Some("s2"), Some("a")).as_array().unwrap().len(),
            1
        );
        assert_eq!(stat(&db, Stat::Builds, None, Some("b")), serde_json::json!([]));
    }

    fn ids(db: &Db, filter: &GameFilter, limit: usize) -> Vec<String> {
        db.games_jsonl(filter, limit)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<GameRecord>(line).unwrap().id)
            .collect()
    }

    #[test]
    fn games_filter_by_team_opponent_and_session_and_page_backwards() {
        let db = league();
        let filter =
            |team: Option<&str>, opponent: Option<&str>, session: Option<&str>, before: Option<&str>| GameFilter {
                team: team.map(Into::into),
                opponent: opponent.map(Into::into),
                session: session.map(Into::into),
                before: before.map(Into::into),
            };
        assert_eq!(
            ids(&db, &filter(Some("a"), None, None, None), 10),
            ["g1", "g2", "g3", "g4"]
        );
        assert_eq!(ids(&db, &filter(Some("c"), None, None, None), 10), ["g3"]);
        assert_eq!(
            ids(&db, &filter(Some("a"), Some("b"), Some("s1"), None), 10),
            ["g1", "g2"]
        );
        assert_eq!(ids(&db, &filter(Some("a"), None, None, Some("g3")), 1), ["g2"]);
        assert!(ids(&db, &filter(None, None, None, Some("absente")), 10).is_empty());
    }

    #[test]
    fn ratings_follow_the_games_of_the_team() {
        let db = league();
        let rating = |game: &str, all: f64| Rating {
            game: game.into(),
            team: "a".into(),
            all: [all, 16.0],
            session: [all - 10.0, 8.0],
        };
        db.add_ratings([rating("g3", 1532.0), rating("g1", 1516.0)]).unwrap();
        let ratings = stat(&db, Stat::Ratings, None, Some("a"));
        assert_eq!(
            ratings[0],
            serde_json::json!({"game": "g1", "ts": "2026-10-24T12:00:00.000Z", "session": "s1",
                "all": {"rating": 1516.0, "delta": 16.0}, "in_session": {"rating": 1506.0, "delta": 8.0}})
        );
        assert_eq!(ratings[1]["game"], "g3");
        assert_eq!(
            stat(&db, Stat::Ratings, Some("s2"), Some("a"))
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(stat(&db, Stat::Ratings, None, Some("b")), serde_json::json!([]));
    }

    #[test]
    fn move_times_skip_the_first_move_when_counting_near_the_limit() {
        let db = league();
        let mut slow = game("g5", "a", "b", Color::Red);
        slow.red.build = Some("h1".into());
        slow.node = Some("node-a".into());
        slow.result.move_ms = vec![900, 20, 900];
        db.insert_game(&slow).unwrap();
        assert_eq!(
            stat(&db, Stat::MoveTimes, None, Some("a")),
            serde_json::json!([
                {"build": "h1", "moves": 3, "avg_ms": 367, "max_ms": 600, "near_limit": 1},
                {"build": "h2", "moves": 2, "avg_ms": 20, "max_ms": 30, "near_limit": 0},
            ])
        );
    }

    #[test]
    fn submissions_hide_the_detail_of_ladder_errors() {
        let db = memory();
        for (verdict, detail) in [
            ("accepted", ""),
            ("rejected", "timeout"),
            ("error", "/srv/state: denied"),
        ] {
            db.insert_submission(&Submission {
                ts: "2026-10-24T12:00:00.000Z".into(),
                team: "a".into(),
                hash: "h1".into(),
                verdict: verdict.into(),
                detail: detail.into(),
            })
            .unwrap();
        }
        let submissions = stat(&db, Stat::Submissions, None, Some("a"));
        assert_eq!(submissions[1]["detail"], "timeout");
        assert_eq!(
            submissions[2],
            serde_json::json!({"ts": "2026-10-24T12:00:00.000Z", "hash": "h1", "verdict": "error", "detail": ""})
        );
        assert_eq!(stat(&db, Stat::Submissions, None, Some("b")), serde_json::json!([]));
    }
}
