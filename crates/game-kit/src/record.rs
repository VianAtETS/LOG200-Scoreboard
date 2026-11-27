//! `GameResult` est ce que l'arbitre écrit sur stdout, `GameRecord` la ligne du JSONL, complétée
//! par le ladder. Les lignes de version inconnue sont ignorées. La validation d'un résultat
//! (`GameResult::validate`) rejoue la partie avec les règles du jeu (`crate::game::Game`).

use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game::{Change, Ending, Game};
use crate::replay::{Replay, ReplayError};
use crate::side::{ByColor, Color};

/// Version écrite. La v2 ajoute `game`, `game_version`, les nulles (`winner` à `null`) et les raisons
/// de fin propres au jeu ; une ligne v1 se lit telle quelle.
pub const RECORD_VERSION: u32 = 2;
/// Versions lues.
pub const RECORD_VERSIONS: std::ops::RangeInclusive<u32> = 1..=RECORD_VERSION;

/// Comment la partie s'est terminée : un abandon, commun à tous les jeux, ou une fin selon les
/// règles. `NoMoves` est la fin d'Isolation (et de bien d'autres jeux) ; les autres fins d'un jeu
/// sont des `Game`, déclarées avec leur libellé dans son descripteur (`reasons`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Reason {
    /// Le perdant n'avait plus de coup.
    NoMoves,
    Timeout,
    /// Connexion fermée ou processus terminé en cours de partie.
    Disconnect,
    /// Jamais de coup valide malgré les `4` (voir `--max-invalid`).
    Protocol,
    /// Jamais connecté (jar qui plante au démarrage, conteneur tué...).
    NoShow,
    /// Autre fin selon les règles du jeu (`king_captured`, `repetition`...), en `snake_case`.
    Game(String),
}

impl Reason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::NoMoves => "no_moves",
            Self::Timeout => "timeout",
            Self::Disconnect => "disconnect",
            Self::Protocol => "protocol",
            Self::NoShow => "no_show",
            Self::Game(name) => name,
        }
    }

    /// Un joueur a perdu sans que les règles aient tranché : le temps, la connexion, le protocole.
    pub const fn is_forfeit(&self) -> bool {
        matches!(self, Self::Timeout | Self::Disconnect | Self::Protocol | Self::NoShow)
    }
}

impl From<&str> for Reason {
    fn from(name: &str) -> Self {
        match name {
            "no_moves" => Self::NoMoves,
            "timeout" => Self::Timeout,
            "disconnect" => Self::Disconnect,
            "protocol" => Self::Protocol,
            "no_show" => Self::NoShow,
            other => Self::Game(other.to_string()),
        }
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Reason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Reason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        if name.is_empty() {
            return Err(serde::de::Error::custom("raison vide"));
        }
        Ok(Self::from(name.as_str()))
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for Reason {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(
                "no_moves, timeout, disconnect, protocol, no_show, ou une fin propre au jeu (descripteur)",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for Reason {}

pub type InvalidCounts = ByColor<u32>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GameResult {
    pub v: u32,
    /// Le jeu et la version de ses règles, comme dans son descripteur. Absents en v1 : Isolation 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<u32>,
    pub time_ms: u64,
    /// Demi-coups imposés par l'arbitre avant de donner la main aux joueurs.
    #[serde(default)]
    pub opening: Vec<String>,
    /// Coups joués par les clients, en notation canonique.
    pub moves: Vec<String>,
    /// `None` : partie nulle (jamais pour un abandon).
    pub winner: Option<Color>,
    pub reason: Reason,
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = ByColor<u32>))]
    pub invalid: InvalidCounts,
    pub duration_ms: u64,
    /// Temps de réflexion de chaque coup de `moves`, même index. Absent chez d'autres arbitres.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub move_ms: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RefereeEvent {
    Ready,
    Move {
        #[serde(rename = "move")]
        mv: String,
        ms: u64,
        /// Effets du coup sur le plateau, dans l'ordre. Absents chez les arbitres d'avant la v2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        changes: Vec<Change>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Player {
    pub team: String,
    /// Identifie la version du jar : un nouveau hash marque une nouvelle soumission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// Joueur de référence : rating fixe dans le classement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchor: bool,
    /// Joué depuis la machine de l'équipe : code non vérifié, `build` déclaré par l'équipe.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remote: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GameRecord {
    pub id: String,
    pub session: String,
    pub ts: String,
    pub red: Player,
    pub black: Player,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tournament: Option<TournamentGame>,
    /// Nœud qui a joué la partie ; absent : le serveur principal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Mesures publiques des bots du ladder ; absent sans bot mesuré.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measures: Option<Measures>,
    #[serde(flatten)]
    pub result: GameResult,
}

/// Mesures d'une partie, une par coup de la couleur, dans l'ordre de ses coups : seulement pour un
/// bot du ladder, dont le code est public.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Measures {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub red: Vec<Measure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub black: Vec<Measure>,
}

/// La ligne `ARENA` d'un coup.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Measure {
    /// Dernière profondeur terminée ; absente sans recherche (coup forcé, sonde, bot sans recherche).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Positions visitées pendant la recherche du coup.
    pub nodes: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TournamentGame {
    pub id: String,
    #[serde(rename = "match")]
    pub match_id: String,
    pub game: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error(transparent)]
    Replay(#[from] ReplayError),
    #[error("no_show annoncé après des coups joués")]
    NoShowAfterMoves,
    #[error("fin annoncée différente du rejeu : {replayed:?}")]
    WrongEnding { replayed: Option<Ending> },
    #[error("partie déjà terminée avant l'abandon annoncé")]
    ForfeitAfterEnd,
    #[error("le gagnant {0} était au trait")]
    WinnerToMove(Color),
    #[error("abandon ({0}) sans gagnant")]
    ForfeitWithoutWinner(Reason),
}

impl GameResult {
    /// `None` pour une nulle.
    pub fn loser(&self) -> Option<Color> {
        self.winner.map(Color::other)
    }

    /// Points de `color` : 1 pour une victoire, 0,5 pour une nulle, 0 pour une défaite.
    pub fn score(&self, color: Color) -> f64 {
        match self.winner {
            Some(winner) if winner == color => 1.0,
            Some(_) => 0.0,
            None => 0.5,
        }
    }

    /// Coups légaux et résultat cohérent avec le rejeu par les règles de `game`.
    pub fn validate<G: Game>(&self, game: &G) -> Result<(), ValidationError> {
        self.replay(game).map(drop)
    }

    /// Comme `validate`, et rend le rejeu, avec les effets de chaque coup, ouverture comprise.
    pub fn replay<G: Game>(&self, game: &G) -> Result<Replay<G>, ValidationError> {
        let all = self.opening.iter().chain(&self.moves).map(String::as_str);
        let replay = Replay::from_notation(game, all)?;
        let ending = replay.ending.as_ref();
        if self.reason.is_forfeit() {
            let Some(winner) = self.winner else {
                return Err(ValidationError::ForfeitWithoutWinner(self.reason.clone()));
            };
            if self.reason == Reason::NoShow {
                // Avant le premier coup client : l'ouverture peut avoir été jouée.
                return if self.moves.is_empty() {
                    Ok(replay)
                } else {
                    Err(ValidationError::NoShowAfterMoves)
                };
            }
            // Abandon : c'est le joueur au trait qui perd.
            return if ending.is_some() {
                Err(ValidationError::ForfeitAfterEnd)
            } else if winner == replay.turn {
                Err(ValidationError::WinnerToMove(replay.turn))
            } else {
                Ok(replay)
            };
        }
        // Fin selon les règles du jeu : le rejeu doit arriver au même verdict, nulle comprise.
        match ending {
            Some(end) if end.winner == self.winner && end.reason == self.reason => Ok(replay),
            _ => Err(ValidationError::WrongEnding {
                replayed: ending.cloned(),
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum SkipReason {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("version non gérée : {0:?}")]
    Version(Option<u64>),
}

#[derive(Debug)]
pub struct SkippedLine {
    /// À partir de 1.
    pub line: usize,
    pub reason: SkipReason,
}

/// Contenu d'un JSONL. Les lignes illisibles, comme une dernière ligne en cours d'écriture, sont
/// mises de côté plutôt que de faire échouer la lecture.
#[derive(Debug, Default)]
pub struct Jsonl {
    pub games: Vec<GameRecord>,
    pub at: Vec<Range<u64>>,
    pub skipped: Vec<SkippedLine>,
}

impl Jsonl {
    pub fn parse(text: &str) -> Self {
        let mut jsonl = Self::default();
        let mut offset = 0;
        for (index, raw) in text.split_inclusive('\n').enumerate() {
            let start = offset;
            offset += raw.len() as u64;
            let line = raw.trim_end_matches(['\n', '\r']);
            if line.trim().is_empty() {
                continue;
            }
            match parse_line(line) {
                Ok(game) => {
                    jsonl.games.push(game);
                    jsonl.at.push(start..offset);
                }
                Err(reason) => jsonl.skipped.push(SkippedLine {
                    line: index + 1,
                    reason,
                }),
            }
        }
        jsonl
    }
}

fn parse_line(line: &str) -> Result<GameRecord, SkipReason> {
    let value: serde_json::Value = serde_json::from_str(line)?;
    match value.get("v").and_then(serde_json::Value::as_u64) {
        Some(v) if u32::try_from(v).is_ok_and(|v| RECORD_VERSIONS.contains(&v)) => Ok(serde_json::from_value(value)?),
        other => Err(SkipReason::Version(other)),
    }
}

pub fn now_rfc3339() -> String {
    rfc3339_at(SystemTime::now())
}

pub fn rfc3339_at(time: SystemTime) -> String {
    let elapsed = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    rfc3339(
        i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
        elapsed.subsec_millis(),
    )
}

fn rfc3339(secs: i64, millis: u32) -> String {
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Jours -> date civile, algorithme de H. Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GameRecord {
        let mv = "G1-F2-C12".to_string();
        GameRecord {
            id: "g1".into(),
            session: "test".into(),
            ts: "2026-10-24T12:00:00.000Z".into(),
            red: Player {
                team: "eq01".into(),
                build: Some("abc".into()),
                anchor: false,
                remote: false,
            },
            black: Player {
                team: "eq02".into(),
                build: None,
                anchor: true,
                remote: false,
            },
            tournament: None,
            node: None,
            measures: None,
            result: GameResult {
                v: RECORD_VERSION,
                game: None,
                game_version: None,
                time_ms: 500,
                opening: vec![],
                moves: vec![mv],
                winner: Some(Color::Red),
                reason: Reason::Timeout,
                invalid: InvalidCounts { red: 0, black: 1 },
                duration_ms: 1234,
                move_ms: vec![42],
            },
        }
    }

    #[test]
    fn jsonl_roundtrip_skips_truncated_and_unknown_versions() {
        let line = serde_json::to_string(&sample()).unwrap();
        let text = format!("{line}\n{{\"v\":3}}\n{}", &line[..line.len() / 2]);
        let jsonl = Jsonl::parse(&text);
        assert_eq!(jsonl.games, vec![sample()]);
        assert_eq!((jsonl.at.len(), jsonl.at[0].clone()), (1, 0..line.len() as u64 + 1));
        assert_eq!(jsonl.skipped.iter().map(|s| s.line).collect::<Vec<_>>(), [2, 3]);
        assert!(matches!(jsonl.skipped[0].reason, SkipReason::Version(Some(3))));
        assert!(matches!(jsonl.skipped[1].reason, SkipReason::Json(_)));
    }

    #[test]
    fn a_v1_line_still_reads_and_a_v2_draw_has_a_null_winner() {
        let v1 = r#"{"id":"g1","session":"s","ts":"t","red":{"team":"a"},"black":{"team":"b"},"v":1,"time_ms":500,"opening":[],"moves":["G1-F2-C12"],"winner":"black","reason":"no_moves","invalid":{"red":0,"black":0},"duration_ms":1}"#;
        let old = Jsonl::parse(v1).games.pop().unwrap();
        assert_eq!(
            (old.result.winner, &old.result.reason),
            (Some(Color::Black), &Reason::NoMoves)
        );
        assert_eq!(old.result.game, None);

        let mut draw = sample();
        draw.result.game = Some("hnefatafl".into());
        draw.result.game_version = Some(1);
        draw.result.winner = None;
        draw.result.reason = Reason::Game("repetition".into());
        let line = serde_json::to_string(&draw).unwrap();
        assert!(line.contains(r#""v":2,"game":"hnefatafl","game_version":1,"#), "{line}");
        assert!(line.contains(r#""winner":null,"reason":"repetition""#), "{line}");
        assert_eq!(Jsonl::parse(&line).games, vec![draw]);
    }

    #[test]
    fn reasons_are_plain_strings_and_forfeits_are_the_platform_ones() {
        for (name, forfeit) in [
            ("no_moves", false),
            ("timeout", true),
            ("disconnect", true),
            ("protocol", true),
            ("no_show", true),
            ("king_captured", false),
        ] {
            let reason = Reason::from(name);
            assert_eq!(serde_json::to_value(&reason).unwrap(), name);
            assert_eq!(serde_json::from_value::<Reason>(name.into()).unwrap(), reason);
            assert_eq!(reason.is_forfeit(), forfeit, "{name}");
        }
        assert_eq!(Reason::from("king_captured"), Reason::Game("king_captured".into()));
        assert!(serde_json::from_value::<Reason>("".into()).is_err());
    }

    #[test]
    fn jsonl_line_is_stable() {
        let game = sample();
        let expected = format!(
            r#"{{"id":"g1","session":"test","ts":"2026-10-24T12:00:00.000Z","red":{{"team":"eq01","build":"abc"}},"black":{{"team":"eq02","anchor":true}},"v":2,"time_ms":500,"opening":[],"moves":["{}"],"winner":"red","reason":"timeout","invalid":{{"red":0,"black":1}},"duration_ms":1234,"move_ms":[42]}}"#,
            game.result.moves[0]
        );
        assert_eq!(serde_json::to_string(&game).unwrap(), expected);
    }

    #[test]
    fn a_record_without_node_round_trips_unchanged() {
        let line = serde_json::to_string(&sample()).unwrap();
        let back: GameRecord = serde_json::from_str(&line).unwrap();
        assert_eq!(back.node, None);
        assert_eq!(serde_json::to_string(&back).unwrap(), line);
        let on_node = GameRecord {
            node: Some("node-a".into()),
            ..sample()
        };
        let line = serde_json::to_string(&on_node).unwrap();
        assert!(line.contains(r#""node":"node-a""#));
        assert_eq!(serde_json::from_str::<GameRecord>(&line).unwrap(), on_node);
    }

    #[test]
    fn measures_are_absent_unless_a_bot_was_measured() {
        assert!(!serde_json::to_string(&sample()).unwrap().contains("measures"));
        let measured = GameRecord {
            measures: Some(Measures {
                red: vec![
                    Measure {
                        depth: Some(6),
                        nodes: 1234,
                    },
                    Measure { depth: None, nodes: 0 },
                ],
                black: Vec::new(),
            }),
            ..sample()
        };
        let line = serde_json::to_string(&measured).unwrap();
        assert!(
            line.contains(r#""measures":{"red":[{"depth":6,"nodes":1234},{"nodes":0}]}"#),
            "{line}"
        );
        assert_eq!(serde_json::from_str::<GameRecord>(&line).unwrap(), measured);
    }

    #[test]
    fn flattened_fields_are_top_level() {
        let v: serde_json::Value = serde_json::to_value(sample()).unwrap();
        assert_eq!(v["winner"], "red");
        assert_eq!(v["reason"], "timeout");
        assert_eq!(v["red"]["team"], "eq01");
        assert!(v["black"].get("build").is_none());
        assert_eq!(v["black"]["anchor"], true);
        assert!(v["red"].get("anchor").is_none());
    }

    #[test]
    fn remote_is_written_only_when_set() {
        let mut game = sample();
        assert!(serde_json::to_value(&game).unwrap()["red"].get("remote").is_none());
        game.red.remote = true;
        let line = serde_json::to_string(&game).unwrap();
        assert!(line.contains(r#""red":{"team":"eq01","build":"abc","remote":true}"#));
        assert_eq!(Jsonl::parse(&line).games, vec![game]);
    }

    #[test]
    fn tournament_is_written_only_when_set() {
        let mut game = sample();
        assert!(serde_json::to_value(&game).unwrap().get("tournament").is_none());
        game.tournament = Some(TournamentGame {
            id: "tournoi-2026-11-08".into(),
            match_id: "W3".into(),
            game: 2,
        });
        let line = serde_json::to_string(&game).unwrap();
        assert!(line.contains(r#""tournament":{"id":"tournoi-2026-11-08","match":"W3","game":2}"#));
        assert_eq!(Jsonl::parse(&line).games, vec![game]);
    }

    #[test]
    fn rfc3339_known_dates() {
        assert_eq!(rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_793_017_330, 42), "2026-10-26T12:22:10.042Z");
        assert_eq!(rfc3339(951_782_400, 0), "2000-02-29T00:00:00.000Z");
    }
}

#[cfg(test)]
mod validation {
    use super::*;
    use crate::game::toy::{PLIES, Toy};

    fn result(plies: u8, winner: Option<Color>, reason: &str) -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            game: Some("toy".into()),
            game_version: Some(1),
            time_ms: 100,
            opening: vec![],
            moves: (0..plies).map(|n| n.to_string()).collect(),
            winner,
            reason: Reason::from(reason),
            invalid: InvalidCounts::default(),
            duration_ms: 1,
            move_ms: vec![],
        }
    }

    #[test]
    fn a_draw_validates_only_as_the_rules_end_it() {
        assert_eq!(result(PLIES, None, "max_plies").validate(&Toy), Ok(()));
        let replayed = Some(Ending {
            winner: None,
            reason: Reason::Game("max_plies".into()),
        });
        for wrong in [
            result(PLIES, Some(Color::Red), "max_plies"),
            result(PLIES, None, "repetition"),
            result(PLIES, None, "no_moves"),
        ] {
            assert_eq!(
                wrong.validate(&Toy),
                Err(ValidationError::WrongEnding {
                    replayed: replayed.clone()
                })
            );
        }
        assert_eq!(
            result(1, None, "max_plies").validate(&Toy),
            Err(ValidationError::WrongEnding { replayed: None }),
            "pas de nulle avant la fin"
        );
    }

    #[test]
    fn a_forfeit_always_has_a_winner() {
        assert_eq!(
            result(1, None, "timeout").validate(&Toy),
            Err(ValidationError::ForfeitWithoutWinner(Reason::Timeout))
        );
        assert_eq!(result(1, Some(Color::Red), "timeout").validate(&Toy), Ok(()));
        assert_eq!(
            result(0, None, "no_show").validate(&Toy),
            Err(ValidationError::ForfeitWithoutWinner(Reason::NoShow))
        );
    }
}
