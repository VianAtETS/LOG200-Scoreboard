//! Les deux camps. `red` et `black` identifient le premier et le second joueur (l'ordre de
//! connexion), quel que soit le jeu : le nom affiché vient du descripteur.

use std::fmt;
use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Color {
    /// Premier joueur : il joue le premier coup.
    Red,
    Black,
}

impl Color {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Red => Self::Black,
            Self::Black => Self::Red,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Black => "black",
        }
    }

    /// Valeur du protocole officiel (message `0`).
    pub const fn code(self) -> u8 {
        match self {
            Self::Red => 0,
            Self::Black => 1,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Une valeur par camp, indexée par `Color`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ByColor<T> {
    pub red: T,
    pub black: T,
}

impl<T> Index<Color> for ByColor<T> {
    type Output = T;

    fn index(&self, color: Color) -> &T {
        match color {
            Color::Red => &self.red,
            Color::Black => &self.black,
        }
    }
}

impl<T> IndexMut<Color> for ByColor<T> {
    fn index_mut(&mut self, color: Color) -> &mut T {
        match color {
            Color::Red => &mut self.red,
            Color::Black => &mut self.black,
        }
    }
}
