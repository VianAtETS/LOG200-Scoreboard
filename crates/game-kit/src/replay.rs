//! Rejeu d'une partie depuis la position initiale, avec les effets de chaque coup.

use thiserror::Error;

use crate::game::{Change, Ending, Game};
use crate::side::Color;

/// Les numéros de coup des messages commencent à 1.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("coup {} illisible : {text:?}", .index + 1)]
    Unreadable { index: usize, text: String },
    #[error("coup {} illégal : {mv}", .index + 1)]
    Illegal { index: usize, mv: String },
    #[error("coup {} joué après la fin de la partie : {mv}", .index + 1)]
    AfterEnd { index: usize, mv: String },
}

impl ReplayError {
    /// Position du coup fautif, à partir de 0.
    pub const fn index(&self) -> usize {
        match self {
            Self::Unreadable { index, .. } | Self::Illegal { index, .. } | Self::AfterEnd { index, .. } => *index,
        }
    }
}

pub struct Replay<G: Game> {
    pub state: G::State,
    pub turn: Color,
    /// Renseigné si le dernier coup a terminé la partie.
    pub ending: Option<Ending>,
    /// Effets de chaque coup joué, dans l'ordre.
    pub changes: Vec<Vec<Change>>,
}

impl<G: Game> Clone for Replay<G> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            turn: self.turn,
            ending: self.ending,
            changes: self.changes.clone(),
        }
    }
}

impl<G: Game> Replay<G> {
    pub fn new(game: &G) -> Self {
        Self {
            state: game.initial(),
            turn: Color::Red,
            ending: None,
            changes: Vec::new(),
        }
    }

    pub fn from_moves(game: &G, moves: impl IntoIterator<Item = G::Move>) -> Result<Self, ReplayError> {
        let mut replay = Self::new(game);
        for mv in moves {
            replay.play(game, mv)?;
        }
        Ok(replay)
    }

    pub fn from_notation<'a>(game: &G, moves: impl IntoIterator<Item = &'a str>) -> Result<Self, ReplayError> {
        let mut replay = Self::new(game);
        for text in moves {
            let index = replay.changes.len();
            let mv = text.parse().map_err(|_| ReplayError::Unreadable {
                index,
                text: text.to_string(),
            })?;
            replay.play(game, mv)?;
        }
        Ok(replay)
    }

    /// Joue le coup suivant et rend ses effets.
    pub fn play(&mut self, game: &G, mv: G::Move) -> Result<&[Change], ReplayError> {
        let index = self.changes.len();
        if self.ending.is_some() {
            return Err(ReplayError::AfterEnd {
                index,
                mv: mv.to_string(),
            });
        }
        if !game.is_legal(&self.state, self.turn, mv) {
            return Err(ReplayError::Illegal {
                index,
                mv: mv.to_string(),
            });
        }
        let changes = game.apply(&mut self.state, self.turn, mv);
        self.ending = game.ending(&self.state, self.turn);
        self.turn = self.turn.other();
        self.changes.push(changes);
        Ok(self.changes.last().map_or(&[], Vec::as_slice))
    }
}
