//! Ce que la page et le ladder savent d'un jeu sans en connaître les règles : identité, plateau,
//! légende des cases. Figé par `(game, version)` : changer les règles ou l'affichage d'un jeu,
//! c'est monter sa version.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::side::{ByColor, Color};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Descriptor {
    /// Identifiant stable, en minuscules : `isolation`, `hnefatafl`.
    pub game: String,
    pub version: u32,
    /// Nom affiché.
    pub name: String,
    pub rows: u8,
    pub cols: u8,
    pub sides: ByColor<SideInfo>,
    /// Position initiale, `rows * cols` codes dans l'ordre du protocole (ligne du haut d'abord).
    pub initial: Vec<u8>,
    pub pieces: Vec<Piece>,
    /// Cases particulières (trône, coins...), dessinées sous les pièces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub special: Vec<Special>,
    /// Raisons de fin propres au jeu (`no_moves`), avec leur libellé.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reasons: BTreeMap<String, String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SideInfo {
    pub label: String,
}

/// Une valeur de case et son dessin.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Piece {
    pub code: u8,
    pub label: String,
    /// Nom que la page traduit en dessin (`empty`, `hole`, `pawn`, `king`...). Un style inconnu
    /// est dessiné comme une pièce générique de son camp.
    pub style: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Color>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Special {
    pub cell: u16,
    pub style: String,
}

impl Descriptor {
    pub fn cells(&self) -> usize {
        usize::from(self.rows) * usize::from(self.cols)
    }

    /// Incohérences qui rendraient le descripteur inutilisable par la page.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.initial.len() != self.cells() {
            problems.push(format!(
                "{} cases initiales au lieu de {}",
                self.initial.len(),
                self.cells()
            ));
        }
        for code in &self.initial {
            if !self.pieces.iter().any(|p| p.code == *code) {
                problems.push(format!("code {code} sans pièce dans la légende"));
            }
        }
        for special in &self.special {
            if usize::from(special.cell) >= self.cells() {
                problems.push(format!("case spéciale {} hors du plateau", special.cell));
            }
        }
        problems.dedup();
        problems
    }
}
