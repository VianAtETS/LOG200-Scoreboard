//! Isolation (LOG200) : plateau, règles et protocole, et leur implémentation de `game_kit::Game`
//! (`game::Isolation`). Les formats communs à tous les jeux sont dans `game_kit` ; `digest`,
//! `record` et `remote` sont réexportés pour les chemins historiques. Formats : docs/formats.md.

pub mod board;
pub mod game;
pub mod notation;
pub mod protocol;
pub mod rules;

pub use game::Isolation;
pub use game_kit::{digest, record, remote};
