//! Messages entre le ladder et un nœud, une trame WebSocket texte par message (JSON). Un jar suit son
//! annonce `jar` dans une trame binaire. Voir docs/formats.md, « Nœuds ».

use serde::{Deserialize, Serialize};

use isolation_core::board::{ByColor, Color};
use isolation_core::record::{GameResult, RefereeEvent};

use crate::arena::GameError;

/// Change à chaque modification incompatible des messages ; un nœud d'un autre protocole est refusé.
pub const PROTOCOL: u32 = 1;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FromNode {
    /// Premier message : qui est le nœud et combien de slots il offre.
    Hello {
        node: String,
        version: String,
        protocol: u32,
        arch: String,
        slots: usize,
    },
    /// Le nœud accepte des parties (sonde réussie, température correcte) ou plus.
    Ready {
        ready: bool,
        why: Option<String>,
    },
    Heartbeat {
        running: usize,
        temp_c: Option<f64>,
    },
    /// Un événement de l'arbitre, pour le direct.
    Event {
        game: String,
        event: RefereeEvent,
    },
    /// Le jar de ce sha256 n'est pas en cache : l'envoyer.
    Need {
        sha256: String,
    },
    /// Fin de partie, avec la sortie d'erreur de chaque joueur (tronquée comme au ladder).
    Result {
        game: String,
        outcome: Outcome,
        logs: ByColor<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToNode {
    Start(WireGame),
    /// Annonce la trame binaire qui suit : le jar de ce sha256.
    Jar {
        sha256: String,
        size: u64,
    },
    Abort {
        game: String,
    },
    /// Le ladder s'arrête : finir les parties en cours, n'en accepter aucune.
    Drain,
}

/// Une partie à jouer sur le nœud. Le nœud construit les commandes depuis sa propre config.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WireGame {
    pub game: String,
    pub slot: usize,
    pub time_ms: u64,
    /// Coups imposés, en notation.
    pub opening: Vec<String>,
    pub timeout_ms: u64,
    pub red: WirePlayer,
    pub black: WirePlayer,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WirePlayer {
    /// Bot de référence, par son nom et ses options (`--margin-ms 100`).
    Bot { strategy: String, args: Vec<String> },
    /// Jar d'équipe, par son contenu.
    Jar { sha256: String },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Ok {
        result: GameResult,
    },
    /// Le bac à sable d'un joueur n'a jamais démarré : la partie sera annulée, comme au ladder.
    Infra {
        side: Color,
        code: i32,
    },
    /// Toute autre panne : partie annulée.
    Error {
        message: String,
    },
}

impl From<&GameError> for Outcome {
    fn from(error: &GameError) -> Self {
        match error {
            GameError::Infra { side, code } => Self::Infra {
                side: *side,
                code: *code,
            },
            other => Self::Error {
                message: other.to_string(),
            },
        }
    }
}

impl Outcome {
    /// Le résultat tel que le ladder l'enregistrera : revalidé, un nœud ne peut pas inventer une fin.
    pub fn into_result(self, node: &str) -> Result<GameResult, GameError> {
        match self {
            Self::Ok { result } => {
                result.validate()?;
                Ok(result)
            }
            Self::Infra { side, code } => Err(GameError::Infra { side, code }),
            Self::Error { message } => Err(GameError::Remote {
                node: node.into(),
                message,
            }),
        }
    }
}

/// 64 chiffres hexadécimaux minuscules : jamais un chemin.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use isolation_core::record::{InvalidCounts, RECORD_VERSION, Reason};

    use super::*;

    fn result() -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            time_ms: 1000,
            opening: vec![],
            moves: vec![],
            winner: Color::Black,
            reason: Reason::Timeout,
            invalid: InvalidCounts::default(),
            duration_ms: 5,
            move_ms: vec![],
        }
    }

    fn sha() -> String {
        "ab".repeat(32)
    }

    #[test]
    fn every_message_round_trips() {
        let from = [
            FromNode::Hello {
                node: "node-a".into(),
                version: "0.30.0".into(),
                protocol: PROTOCOL,
                arch: "aarch64".into(),
                slots: 2,
            },
            FromNode::Ready {
                ready: false,
                why: Some("trop chaud".into()),
            },
            FromNode::Heartbeat {
                running: 1,
                temp_c: Some(61.5),
            },
            FromNode::Event {
                game: "g1".into(),
                event: RefereeEvent::Move {
                    mv: "G1-G4-F9".into(),
                    ms: 3,
                },
            },
            FromNode::Need { sha256: sha() },
            FromNode::Result {
                game: "g1".into(),
                outcome: Outcome::Ok { result: result() },
                logs: ByColor {
                    red: "stderr rouge".into(),
                    black: String::new(),
                },
            },
        ];
        for message in from {
            let line = serde_json::to_string(&message).unwrap();
            assert_eq!(serde_json::from_str::<FromNode>(&line).unwrap(), message, "{line}");
        }
        let to = [
            ToNode::Start(WireGame {
                game: "g1".into(),
                slot: 1,
                time_ms: 1000,
                opening: vec!["G1-G4-F9".into()],
                timeout_ms: 264_000,
                red: WirePlayer::Bot {
                    strategy: "niveau-1".into(),
                    args: vec![],
                },
                black: WirePlayer::Jar { sha256: sha() },
            }),
            ToNode::Jar {
                sha256: sha(),
                size: 42,
            },
            ToNode::Abort { game: "g1".into() },
            ToNode::Drain,
        ];
        for message in to {
            let line = serde_json::to_string(&message).unwrap();
            assert_eq!(serde_json::from_str::<ToNode>(&line).unwrap(), message, "{line}");
        }
    }

    #[test]
    fn the_wire_format_is_stable() {
        let line = serde_json::to_string(&ToNode::Abort { game: "g1".into() }).unwrap();
        assert_eq!(line, r#"{"type":"abort","game":"g1"}"#);
        let line = serde_json::to_string(&FromNode::Need { sha256: sha() }).unwrap();
        assert_eq!(line, format!(r#"{{"type":"need","sha256":"{}"}}"#, sha()));
    }

    #[test]
    fn unknown_message_types_and_fields_are_rejected() {
        assert!(serde_json::from_str::<ToNode>(r#"{"type":"exec","cmd":"rm -rf /"}"#).is_err());
        assert!(serde_json::from_str::<ToNode>(r#"{"type":"abort","game":"g1","cmd":"x"}"#).is_err());
        assert!(serde_json::from_str::<FromNode>(r#"{"type":"ready"}"#).is_err());
    }

    #[test]
    fn outcome_errors_map_back_to_game_errors() {
        let infra = GameError::Infra {
            side: Color::Red,
            code: 125,
        };
        let back = Outcome::from(&infra).into_result("node-a");
        assert!(matches!(
            back,
            Err(GameError::Infra {
                side: Color::Red,
                code: 125
            })
        ));
        let timeout = Outcome::from(&GameError::Timeout(std::time::Duration::from_secs(3)));
        assert!(matches!(timeout.into_result("node-a"), Err(GameError::Remote { .. })));
        let ok = Outcome::Ok { result: result() }.into_result("node-a").unwrap();
        assert_eq!(ok, result());
        let mut wrong = result();
        wrong.winner = Color::Red;
        assert!(
            matches!(
                Outcome::Ok { result: wrong }.into_result("node-a"),
                Err(GameError::Inconsistent(_))
            ),
            "un résultat de nœud est revalidé"
        );
    }

    #[test]
    fn is_sha256_rejects_paths_and_uppercase() {
        assert!(is_sha256(&sha()));
        assert!(!is_sha256(&"AB".repeat(32)));
        assert!(!is_sha256("../../etc/passwd"));
        assert!(!is_sha256(&"ab".repeat(31)));
        assert!(!is_sha256(&format!("{}/", &"ab".repeat(32)[..63])));
    }
}
