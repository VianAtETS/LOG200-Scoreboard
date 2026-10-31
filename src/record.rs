//! `GameResult` est ce que l'arbitre écrit sur stdout, `GameRecord` la ligne du JSONL, complétée
//! par le ladder. Les lignes de version inconnue sont ignorées.

use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::board::{ByColor, Color};
use crate::rules::{Replay, ReplayError};

pub const RECORD_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NoMoves,
    Timeout,
    /// Connexion fermée ou processus terminé en cours de partie.
    Disconnect,
    /// Jamais de coup valide malgré les `4` (voir `--max-invalid`).
    Protocol,
    /// Jamais connecté (jar qui plante au démarrage, conteneur tué...).
    NoShow,
}

pub type InvalidCounts = ByColor<u32>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct GameResult {
    pub v: u32,
    pub time_ms: u64,
    /// Demi-coups imposés par l'arbitre avant de donner la main aux joueurs.
    #[serde(default)]
    pub opening: Vec<String>,
    /// Coups joués par les clients, en notation canonique.
    pub moves: Vec<String>,
    pub winner: Color,
    pub reason: Reason,
    #[serde(default)]
    pub invalid: InvalidCounts,
    pub duration_ms: u64,
    /// Temps de réflexion de chaque coup de `moves`, même index. Absent chez d'autres arbitres.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub move_ms: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
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
pub struct GameRecord {
    pub id: String,
    pub session: String,
    pub ts: String,
    pub red: Player,
    pub black: Player,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tournament: Option<TournamentGame>,
    #[serde(flatten)]
    pub result: GameResult,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
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
    #[error("no_moves annoncé, gagnant rejoué : {replayed:?}")]
    WrongWinner { replayed: Option<Color> },
    #[error("partie déjà terminée avant l'abandon annoncé")]
    ForfeitAfterEnd,
    #[error("le gagnant {0} était au trait")]
    WinnerToMove(Color),
}

impl GameResult {
    /// Coups légaux et résultat cohérent avec le rejeu.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let all = self.opening.iter().chain(&self.moves).map(String::as_str);
        let Replay { turn, winner, .. } = Replay::from_notation(all)?;
        match self.reason {
            Reason::NoShow if !self.moves.is_empty() => Err(ValidationError::NoShowAfterMoves),
            Reason::NoMoves if winner != Some(self.winner) => Err(ValidationError::WrongWinner { replayed: winner }),
            Reason::NoShow | Reason::NoMoves => Ok(()),
            // Abandon : c'est le joueur au trait qui perd.
            Reason::Timeout | Reason::Disconnect | Reason::Protocol => {
                if winner.is_some() {
                    Err(ValidationError::ForfeitAfterEnd)
                } else if self.winner == turn {
                    Err(ValidationError::WinnerToMove(turn))
                } else {
                    Ok(())
                }
            }
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
        Some(v) if v == u64::from(RECORD_VERSION) => Ok(serde_json::from_value(value)?),
        other => Err(SkipReason::Version(other)),
    }
}

/// Heure courante en RFC 3339 UTC, à la milliseconde.
pub fn now_rfc3339() -> String {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
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
    use crate::board::Board;
    use crate::rules::legal_moves;

    fn sample() -> GameRecord {
        let mv = legal_moves(&Board::initial(), Color::Red)[0].to_string();
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
            result: GameResult {
                v: RECORD_VERSION,
                time_ms: 500,
                opening: vec![],
                moves: vec![mv],
                winner: Color::Red,
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
        let text = format!("{line}\n{{\"v\":2}}\n{}", &line[..line.len() / 2]);
        let jsonl = Jsonl::parse(&text);
        assert_eq!(jsonl.games, vec![sample()]);
        assert_eq!((jsonl.at.len(), jsonl.at[0].clone()), (1, 0..line.len() as u64 + 1));
        assert_eq!(jsonl.skipped.iter().map(|s| s.line).collect::<Vec<_>>(), [2, 3]);
        assert!(matches!(jsonl.skipped[0].reason, SkipReason::Version(Some(2))));
        assert!(matches!(jsonl.skipped[1].reason, SkipReason::Json(_)));
    }

    #[test]
    fn jsonl_line_is_stable() {
        let game = sample();
        let expected = format!(
            r#"{{"id":"g1","session":"test","ts":"2026-10-24T12:00:00.000Z","red":{{"team":"eq01","build":"abc"}},"black":{{"team":"eq02","anchor":true}},"v":1,"time_ms":500,"opening":[],"moves":["{}"],"winner":"red","reason":"timeout","invalid":{{"red":0,"black":1}},"duration_ms":1234,"move_ms":[42]}}"#,
            game.result.moves[0]
        );
        assert_eq!(serde_json::to_string(&game).unwrap(), expected);
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
    fn validate_checks_winner_consistency() {
        // Après un coup rouge, c'est au noir : un timeout le fait perdre.
        assert_eq!(sample().result.validate(), Ok(()));
        let mut wrong = sample().result;
        wrong.winner = Color::Black;
        assert_eq!(wrong.validate(), Err(ValidationError::WinnerToMove(Color::Black)));
        let mut no_moves = sample().result;
        no_moves.reason = Reason::NoMoves;
        assert_eq!(
            no_moves.validate(),
            Err(ValidationError::WrongWinner { replayed: None })
        );
    }

    #[test]
    fn no_show_requires_no_moves_and_accepts_either_winner() {
        let mut absent = sample().result;
        absent.reason = Reason::NoShow;
        assert_eq!(absent.validate(), Err(ValidationError::NoShowAfterMoves));
        absent.moves.clear();
        for winner in [Color::Red, Color::Black] {
            absent.winner = winner;
            assert_eq!(absent.validate(), Ok(()), "{winner:?}");
        }
        let v: serde_json::Value = serde_json::to_value(Reason::NoShow).unwrap();
        assert_eq!(v, "no_show");
    }

    #[test]
    fn rfc3339_known_dates() {
        assert_eq!(rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_793_017_330, 42), "2026-10-26T12:22:10.042Z");
        assert_eq!(rfc3339(951_782_400, 0), "2000-02-29T00:00:00.000Z");
    }
}
