//! Ce qu'un jeu fournit à la plateforme. Voir docs/spec-multi-jeux.md.

use std::fmt;
use std::str::FromStr;

use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::descriptor::Descriptor;
use crate::record::Reason;
use crate::side::Color;

/// Règles d'un jeu à deux joueurs qui alternent, le rouge (premier joueur) en premier.
pub trait Game {
    /// Position, y compris ce dont les règles ont besoin en plus du plateau (historique...).
    type State: Clone;
    /// `Display` donne la notation canonique (enregistrements, `--opening`) ; `FromStr` la lit.
    type Move: Copy + Eq + fmt::Display + FromStr;

    fn descriptor(&self) -> Descriptor;

    fn initial(&self) -> Self::State;

    /// Codes des cases, dans l'ordre des messages `1` et `2` du protocole.
    fn cells(&self, state: &Self::State) -> Vec<u8>;

    fn is_legal(&self, state: &Self::State, side: Color, mv: Self::Move) -> bool;

    /// Tous les coups légaux de `side`, dans un ordre stable.
    fn legal_moves(&self, state: &Self::State, side: Color) -> Vec<Self::Move>;

    /// Joue un coup légal de `side` et rend ses effets, à appliquer dans l'ordre.
    fn apply(&self, state: &mut Self::State, side: Color, mv: Self::Move) -> Vec<Change>;

    /// Après un coup de `mover` : la fin de partie, s'il y en a une.
    fn ending(&self, state: &Self::State, mover: Color) -> Option<Ending>;

    /// Format des messages `3` et `5` du serveur officiel.
    fn protocol_move(&self, mv: Self::Move) -> String {
        mv.to_string()
    }

    /// `plies` demi-coups légaux tirés au hasard depuis la position initiale.
    fn random_opening<R: Rng>(&self, plies: usize, rng: &mut R) -> Vec<Self::Move> {
        let mut state = self.initial();
        let mut turn = Color::Red;
        let mut opening = Vec::with_capacity(plies);
        for _ in 0..plies {
            let Some(&mv) = self.legal_moves(&state, turn).choose(rng) else {
                break;
            };
            self.apply(&mut state, turn, mv);
            opening.push(mv);
            if self.ending(&state, turn).is_some() {
                break;
            }
            turn = turn.other();
        }
        opening
    }
}

/// Fin de partie selon les règles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ending {
    pub winner: Color,
    pub reason: Reason,
}

/// Une case qui change : son index dans le plateau du protocole et sa nouvelle valeur.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Change {
    pub cell: u16,
    pub code: u8,
    pub role: Role,
}

/// Sert à marquer le dernier coup dans la page.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Case de départ de la pièce jouée.
    From,
    /// Case d'arrivée.
    To,
    /// Toute autre conséquence : case détruite, pièce capturée...
    Effect,
}

impl Change {
    pub fn new(cell: usize, code: u8, role: Role) -> Self {
        Self {
            cell: u16::try_from(cell).unwrap_or(u16::MAX),
            code,
            role,
        }
    }
}
