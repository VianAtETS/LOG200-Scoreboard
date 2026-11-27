//! Isolation (LOG200) : plateau, règles et protocole, et leur implémentation de `game_kit::Game`
//! (`game::Isolation`). Les formats communs à tous les jeux sont dans `game_kit` ; `record` est
//! réexporté pour les chemins historiques. Les formats échangés sont décrits dans docs/formats.md.

pub mod board;
pub mod digest;
pub mod game;
pub mod notation;
pub mod protocol;
pub mod remote;
pub mod rules;

pub use game::Isolation;
pub use game_kit::record;
