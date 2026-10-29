//! Mode distant (docs/formats.md, « Jouer à distance ») : `isolation-remote` relaie le jar d'une
//! équipe, qui tourne sur sa machine, par une WebSocket vers le ladder. Une trame texte JSON par
//! message, dans les deux sens.

use serde::{Deserialize, Serialize};

use crate::board::Color;

pub const PROTOCOL_VERSION: u32 = 1;
pub const PATH: &str = "/api/remote";

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Premier message du client. `build` : comme dans `games.jsonl`, déclaré par l'équipe.
    Hello {
        v: u32,
        build: String,
        client: String,
    },
    Welcome {
        team: String,
        time_ms: u64,
    },
    /// Le client lance son jar, qui se connecte à lui comme au serveur officiel. Sans l'adversaire :
    /// une équipe ne peut pas choisir ses parties.
    Game {
        id: String,
        side: Color,
        time_ms: u64,
    },
    /// Le jar est connecté au relais : le ladder le connecte à l'arbitre.
    Ready,
    /// Le jar n'a pas pu être lancé ; la session se termine.
    Failed {
        reason: String,
    },
    /// Le jar a fermé sa connexion en cours de partie : le ladder ferme la sienne avec l'arbitre.
    Hangup,
    /// Une ligne du protocole des clients, sans fin de ligne.
    Line {
        text: String,
    },
    /// L'arbitre a fermé la connexion de ce joueur : la partie est finie pour lui.
    End {
        id: String,
    },
}

impl Message {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// 16 caractères hexadécimaux en minuscules : le début du sha256 du jar.
pub fn is_build(build: &str) -> bool {
    build.len() == 16 && build.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_tagged_json() {
        let game = Message::Game {
            id: "g1".into(),
            side: Color::Black,
            time_ms: 500,
        };
        assert_eq!(
            game.to_json(),
            r#"{"type":"game","id":"g1","side":"black","time_ms":500}"#
        );
        assert_eq!(Message::Ready.to_json(), r#"{"type":"ready"}"#);
        for message in [
            game,
            Message::Line {
                text: "G1-G4-F9".into(),
            },
        ] {
            assert_eq!(Message::from_json(&message.to_json()).unwrap(), message);
        }
        assert!(Message::from_json(r#"{"type":"unknown"}"#).is_err());
    }

    #[test]
    fn builds_are_short_lowercase_hashes() {
        assert!(is_build("3fa9c2d1e0b4a7f6"));
        for bad in ["3FA9C2D1E0B4A7F6", "3fa9c2d1e0b4a7f", "3fa9c2d1e0b4a7fg", ""] {
            assert!(!is_build(bad), "{bad}");
        }
    }
}
