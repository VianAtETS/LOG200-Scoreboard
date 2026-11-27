//! Ce qu'un jeu fournit à la plateforme et ce qu'il en reçoit : le trait `Game`, le descripteur,
//! le cadre du protocole du serveur officiel, les formats des parties et un arbitre générique.
//! Voir docs/spec-multi-jeux.md ; les formats échangés sont décrits dans docs/formats.md.

pub mod descriptor;
pub mod game;
pub mod protocol;
pub mod record;
pub mod referee;
pub mod replay;
pub mod side;

pub use descriptor::Descriptor;
pub use game::{Change, Ending, Game, Role};
pub use side::{ByColor, Color};
