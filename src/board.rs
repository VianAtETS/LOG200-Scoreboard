//! Plateau 12×12. `x` est la colonne (A = 0) et `y` la ligne comptée depuis le haut : la ligne
//! affichée "12" a `y = 0`. L'index `y * 12 + x` est l'ordre des cases dans les messages 1 et 2.

use std::fmt;
use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SIZE: u8 = 12;
pub const CELLS: usize = SIZE as usize * SIZE as usize;

/// Leur ordre fixe celui dans lequel les coups sont générés.
pub const DIRS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
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
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Une valeur par couleur, indexée par `Color`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
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

/// Contenu d'une case ; la discriminante est la valeur du protocole.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cell {
    Empty = 0,
    Destroyed = 1,
    Black = 3,
    Red = 5,
}

impl Cell {
    pub const fn pawn(color: Color) -> Self {
        match color {
            Color::Red => Self::Red,
            Color::Black => Self::Black,
        }
    }

    pub const fn color(self) -> Option<Color> {
        match self {
            Self::Red => Some(Color::Red),
            Self::Black => Some(Color::Black),
            Self::Empty | Self::Destroyed => None,
        }
    }

    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn symbol(self) -> char {
        match self {
            Self::Empty => '.',
            Self::Destroyed => '#',
            Self::Red => 'R',
            Self::Black => 'B',
        }
    }
}

impl TryFrom<u8> for Cell {
    type Error = u8;

    fn try_from(code: u8) -> Result<Self, u8> {
        [Self::Empty, Self::Destroyed, Self::Black, Self::Red]
            .into_iter()
            .find(|cell| cell.code() == code)
            .ok_or(code)
    }
}

/// Case du plateau ; le constructeur garantit qu'elle est dedans.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Square {
    x: u8,
    y: u8,
}

impl Square {
    pub const fn new(x: u8, y: u8) -> Option<Self> {
        if x < SIZE && y < SIZE {
            Some(Self { x, y })
        } else {
            None
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        let index = u8::try_from(index).ok()?;
        Self::new(index % SIZE, index / SIZE)
    }

    pub const fn x(self) -> u8 {
        self.x
    }

    pub const fn y(self) -> u8 {
        self.y
    }

    pub fn offset(self, dx: i8, dy: i8) -> Option<Self> {
        Self::new(self.x.checked_add_signed(dx)?, self.y.checked_add_signed(dy)?)
    }

    pub fn index(self) -> usize {
        usize::from(self.y) * usize::from(SIZE) + usize::from(self.x)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", char::from(b'A' + self.x), SIZE - self.y)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub destroyed: Square,
}

impl fmt::Display for Move {
    /// Format canonique compact, celui du JSONL.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}-{}", self.from, self.to, self.destroyed)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BoardError {
    #[error("{0} cases au lieu de {CELLS}")]
    WrongLength(usize),
    #[error("valeur {value} invalide en {square}")]
    InvalidValue { value: u8, square: Square },
    #[error("deux pions {0}")]
    DuplicatePawn(Color),
    #[error("pion {0} manquant")]
    MissingPawn(Color),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Board {
    cells: [Cell; CELLS],
    pawns: ByColor<Square>,
    /// Le N des phases de destruction.
    destroyed: u32,
}

impl Default for Board {
    fn default() -> Self {
        Self::initial()
    }
}

impl Board {
    const RED_START: Square = Square { x: 6, y: 11 };
    const BLACK_START: Square = Square { x: 5, y: 0 };

    /// Rouge en G1, noir en F12.
    pub fn initial() -> Self {
        let mut cells = [Cell::Empty; CELLS];
        cells[Self::RED_START.index()] = Cell::Red;
        cells[Self::BLACK_START.index()] = Cell::Black;
        Self {
            cells,
            pawns: ByColor {
                red: Self::RED_START,
                black: Self::BLACK_START,
            },
            destroyed: 0,
        }
    }

    /// Plateau des messages 1 et 2 : exactement un pion de chaque couleur.
    pub fn from_cells(values: &[u8]) -> Result<Self, BoardError> {
        if values.len() != CELLS {
            return Err(BoardError::WrongLength(values.len()));
        }
        let mut cells = [Cell::Empty; CELLS];
        let mut pawns = ByColor::<Option<Square>>::default();
        let mut destroyed = 0;
        let squares = (0..CELLS).filter_map(Square::from_index);
        for ((&value, cell), square) in values.iter().zip(&mut cells).zip(squares) {
            *cell = Cell::try_from(value).map_err(|value| BoardError::InvalidValue { value, square })?;
            if *cell == Cell::Destroyed {
                destroyed += 1;
            }
            if let Some(color) = cell.color()
                && pawns[color].replace(square).is_some()
            {
                return Err(BoardError::DuplicatePawn(color));
            }
        }
        let pawn = |color| pawns[color].ok_or(BoardError::MissingPawn(color));
        Ok(Self {
            cells,
            pawns: ByColor {
                red: pawn(Color::Red)?,
                black: pawn(Color::Black)?,
            },
            destroyed,
        })
    }

    pub const fn cells(&self) -> &[Cell; CELLS] {
        &self.cells
    }

    pub fn get(&self, square: Square) -> Cell {
        self.cells[square.index()]
    }

    pub fn is_empty(&self, square: Square) -> bool {
        self.get(square) == Cell::Empty
    }

    pub fn pawn(&self, color: Color) -> Square {
        self.pawns[color]
    }

    pub const fn destroyed(&self) -> u32 {
        self.destroyed
    }

    /// Le coup doit être légal (`rules::is_legal`).
    pub fn apply(&mut self, mv: Move) {
        let pawn = self.get(mv.from);
        debug_assert!(pawn.color().is_some(), "pas de pion en {}", mv.from);
        self.cells[mv.from.index()] = Cell::Empty;
        self.cells[mv.to.index()] = pawn;
        self.cells[mv.destroyed.index()] = Cell::Destroyed;
        self.destroyed += 1;
        if let Some(color) = pawn.color() {
            self.pawns[color] = mv.to;
        }
    }

    /// Inverse de `apply` pour le même coup.
    pub fn undo(&mut self, mv: Move) {
        let pawn = self.get(mv.to);
        self.cells[mv.destroyed.index()] = Cell::Empty;
        self.cells[mv.to.index()] = Cell::Empty;
        self.cells[mv.from.index()] = pawn;
        self.destroyed -= 1;
        if let Some(color) = pawn.color() {
            self.pawns[color] = mv.from;
        }
    }
}

impl fmt::Display for Board {
    /// Vue de débogage, ligne 12 en haut.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (y, row) in self.cells.chunks(usize::from(SIZE)).enumerate() {
            write!(f, "{:>2} ", usize::from(SIZE) - y)?;
            for cell in row {
                write!(f, "{}", cell.symbol())?;
            }
            writeln!(f)?;
        }
        write!(f, "   ABCDEFGHIJKL")
    }
}

#[cfg(test)]
pub mod test_support {
    use super::{Board, Cell};

    /// Plateau depuis un schéma de 12 lignes (ligne 12 en premier), avec les symboles de `Cell`.
    /// Les espaces sont ignorés.
    pub fn diagram(text: &str) -> Board {
        let codes: Vec<u8> = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| {
                [Cell::Empty, Cell::Destroyed, Cell::Red, Cell::Black]
                    .into_iter()
                    .find(|cell| cell.symbol() == c)
                    .unwrap_or_else(|| panic!("symbole {c:?}"))
                    .code()
            })
            .collect();
        Board::from_cells(&codes).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: u8, y: u8) -> Square {
        Square::new(x, y).unwrap()
    }

    #[test]
    fn initial_positions_match_protocol_indices() {
        let board = Board::initial();
        assert_eq!(board.pawn(Color::Red).to_string(), "G1");
        assert_eq!(board.pawn(Color::Black).to_string(), "F12");
        assert_eq!(board.cells()[138], Cell::Red);
        assert_eq!(board.cells()[5], Cell::Black);
    }

    #[test]
    fn squares_stay_on_the_board() {
        assert_eq!(Square::new(SIZE, 0), None);
        assert_eq!(square(0, 0).offset(-1, 0), None);
        assert_eq!(square(11, 11).offset(1, 1), None);
        assert_eq!(square(5, 5).offset(-2, 3), Some(square(3, 8)));
        assert_eq!(Square::from_index(CELLS), None);
        assert_eq!(Square::from_index(138), Some(square(6, 11)));
    }

    #[test]
    fn cells_roundtrip() {
        let board = Board::initial();
        let codes: Vec<u8> = board.cells().iter().map(|c| c.code()).collect();
        assert_eq!(Board::from_cells(&codes).unwrap(), board);
    }

    #[test]
    fn apply_then_undo_restores_board() {
        let mut board = Board::initial();
        let mv = Move {
            from: square(6, 11),
            to: square(6, 8),
            destroyed: square(5, 3),
        };
        board.apply(mv);
        assert_eq!(board.pawn(Color::Red), square(6, 8));
        assert_eq!(board.get(square(5, 3)), Cell::Destroyed);
        assert_eq!(board.destroyed(), 1);
        board.undo(mv);
        assert_eq!(board, Board::initial());
    }

    #[test]
    fn from_cells_rejects_bad_boards() {
        assert_eq!(Board::from_cells(&[0; CELLS]), Err(BoardError::MissingPawn(Color::Red)));
        assert_eq!(Board::from_cells(&[0; 3]), Err(BoardError::WrongLength(3)));
        let mut codes = [0; CELLS];
        codes[0] = 5;
        codes[1] = 5;
        assert_eq!(Board::from_cells(&codes), Err(BoardError::DuplicatePawn(Color::Red)));
        codes[1] = 2;
        assert!(matches!(
            Board::from_cells(&codes),
            Err(BoardError::InvalidValue { value: 2, .. })
        ));
    }
}
