//! Hnefatafl 13×13 du laboratoire de LOG320, tel que l'applique le serveur officiel (`BoardGame.exe`,
//! version Hnefatafl) : règles et protocole relevés dans son code, décrits dans docs/rules-hnefatafl.md.
//!
//! Le plateau est la liste des cases du protocole : ligne 13 d'abord (`y = 0`), de A à M (`x`), index
//! `y * 13 + x`. Codes : 0 vide, 2 défenseur (noir), 4 attaquant (rouge), 5 roi (noir). Le trône (G7)
//! et les coins ne sont pas dans le plateau : ce sont des cases fixes.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use game_kit::descriptor::{Descriptor, Piece, SideInfo, Special};
use game_kit::protocol::Dialect;
use game_kit::record::Reason;
use game_kit::{ByColor, Change, Color, Ending, Game, Role};
use thiserror::Error;

pub const SIZE: u8 = 13;
pub const CELLS: usize = SIZE as usize * SIZE as usize;

pub const EMPTY: u8 = 0;
pub const DEFENDER: u8 = 2;
pub const ATTACKER: u8 = 4;
pub const KING: u8 = 5;

/// Le trône, G7.
pub const THRONE: u8 = 6 * SIZE + 6;
/// Les cases de sortie : A13, M13, A1, M1.
pub const CORNERS: [u8; 4] = [0, SIZE - 1, (SIZE - 1) * SIZE, SIZE * SIZE - 1];

/// Version des règles et de l'affichage, celle du descripteur.
pub const VERSION: u32 = 1;

/// Le serveur ne détecte pas les répétitions : « dans le cas où le jeu n'avance plus, ce sera considéré
/// comme un match nul ». Le ladder en fait une règle : nulle après autant de demi-coups.
pub const MAX_PLIES: u32 = 300;

const DIRS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Les cases où seul le roi s'arrête, et qui comptent comme une pièce ennemie pour une capture.
pub fn is_special(cell: u8) -> bool {
    cell == THRONE || CORNERS.contains(&cell)
}

/// Le camp d'une pièce ; `None` pour une case vide.
pub const fn side_of(code: u8) -> Option<Color> {
    match code {
        ATTACKER => Some(Color::Red),
        DEFENDER | KING => Some(Color::Black),
        _ => None,
    }
}

fn step(cell: u8, (dx, dy): (i8, i8)) -> Option<u8> {
    let (x, y) = (
        i16::from(cell % SIZE) + i16::from(dx),
        i16::from(cell / SIZE) + i16::from(dy),
    );
    let size = i16::from(SIZE);
    ((0..size).contains(&x) && (0..size).contains(&y)).then(|| u8::try_from(y * size + x).unwrap_or(0))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub from: u8,
    pub to: u8,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Board {
    cells: [u8; CELLS],
    king: u8,
    /// Demi-coups joués depuis le début, ouverture comprise (`MAX_PLIES`).
    plies: u32,
}

impl Board {
    /// La position de départ de l'énoncé.
    pub fn initial() -> Self {
        let mut cells = [EMPTY; CELLS];
        let mut put = |x: u8, y: u8, code: u8| cells[usize::from(y * SIZE + x)] = code;
        for i in 4..=8 {
            // Rangées du haut et du bas, colonnes A et M : cinq attaquants chacune.
            put(i, 0, ATTACKER);
            put(i, 12, ATTACKER);
            put(0, i, ATTACKER);
            put(12, i, ATTACKER);
        }
        for (x, y) in [(6, 1), (6, 11), (1, 6), (11, 6)] {
            put(x, y, ATTACKER);
        }
        for i in 3..=9 {
            put(i, 6, DEFENDER);
            put(6, i, DEFENDER);
        }
        put(6, 6, KING);
        Self {
            cells,
            king: THRONE,
            plies: 0,
        }
    }

    pub const fn cells(&self) -> &[u8; CELLS] {
        &self.cells
    }

    pub fn get(&self, cell: u8) -> u8 {
        self.cells[usize::from(cell)]
    }

    pub const fn king(&self) -> u8 {
        self.king
    }

    pub const fn plies(&self) -> u32 {
        self.plies
    }

    /// Les arrivées d'une pièce : en ligne droite, jusqu'à une pièce ou au bord. Une pièce autre que le
    /// roi ne s'arrête ni sur le trône ni sur un coin, mais passe par-dessus le trône vide.
    pub fn destinations(&self, from: u8, mut visit: impl FnMut(u8)) {
        let king = self.get(from) == KING;
        for dir in DIRS {
            let mut at = from;
            while let Some(next) = step(at, dir) {
                if self.get(next) != EMPTY {
                    break;
                }
                if king || !is_special(next) {
                    visit(next);
                }
                at = next;
            }
        }
    }

    /// Tous les coups de `side`, par case de départ puis par direction.
    pub fn moves(&self, side: Color) -> Vec<Move> {
        let mut moves = Vec::new();
        for from in 0..u8::try_from(CELLS).unwrap_or(u8::MAX) {
            if side_of(self.get(from)) == Some(side) {
                self.destinations(from, |to| moves.push(Move { from, to }));
            }
        }
        moves
    }

    pub fn has_moves(&self, side: Color) -> bool {
        (0..u8::try_from(CELLS).unwrap_or(u8::MAX)).any(|from| {
            let mut any = false;
            if side_of(self.get(from)) == Some(side) {
                self.destinations(from, |_| any = true);
            }
            any
        })
    }

    /// Joue un coup légal et rend les cases capturées. Seules les captures actives comptent : la pièce
    /// jouée encadre une pièce adverse avec une alliée (le roi compris), le trône ou un coin, même
    /// occupé. Le roi ne se prend pas ainsi, et le bord ne sert pas à prendre une pièce.
    pub fn play(&mut self, mv: Move) -> Vec<u8> {
        let piece = self.get(mv.from);
        self.cells[usize::from(mv.from)] = EMPTY;
        self.cells[usize::from(mv.to)] = piece;
        if piece == KING {
            self.king = mv.to;
        }
        self.plies += 1;
        let Some(mover) = side_of(piece) else {
            return Vec::new();
        };
        let enemy = if mover == Color::Red { DEFENDER } else { ATTACKER };
        let mut captured = Vec::new();
        for dir in DIRS {
            let Some(next) = step(mv.to, dir) else { continue };
            let Some(beyond) = step(next, dir) else { continue };
            if self.get(next) == enemy && (side_of(self.get(beyond)) == Some(mover) || is_special(beyond)) {
                self.cells[usize::from(next)] = EMPTY;
                captured.push(next);
            }
        }
        captured
    }

    /// Roi pris : chacun de ses quatre côtés est un attaquant, le trône, un coin ou le bord. C'est un
    /// état, pas une capture : le serveur le vérifie après chaque coup rouge.
    pub fn king_captured(&self) -> bool {
        DIRS.iter().all(|&dir| match step(self.king, dir) {
            None => true,
            Some(side) => self.get(side) == ATTACKER || is_special(side),
        })
    }

    pub fn king_escaped(&self) -> bool {
        CORNERS.contains(&self.king)
    }

    /// Un plateau reçu : 169 codes connus et un seul roi. Les demi-coups déjà joués sont inconnus.
    pub fn from_cells(values: &[u8]) -> Option<Self> {
        let cells: [u8; CELLS] = values.try_into().ok()?;
        if cells.iter().any(|&c| !matches!(c, EMPTY | DEFENDER | ATTACKER | KING)) {
            return None;
        }
        let mut kings = cells.iter().enumerate().filter(|&(_, &c)| c == KING);
        let (king, _) = kings.next()?;
        if kings.next().is_some() {
            return None;
        }
        Some(Self {
            cells,
            king: u8::try_from(king).ok()?,
            plies: 0,
        })
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::initial()
    }
}

/// Nom d'une case : `G7`, colonne A à M, ligne 1 en bas.
pub fn cell_name(cell: u8) -> String {
    format!("{}{}", char::from(b'A' + cell % SIZE), SIZE - cell / SIZE)
}

impl fmt::Display for Move {
    /// Format canonique compact, celui des enregistrements : `G12-L12`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}", cell_name(self.from), cell_name(self.to))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("coup illisible : {0:?}")]
pub struct NotationError(pub String);

impl FromStr for Move {
    type Err = NotationError;

    /// Comme le serveur : majuscules, et entre les deux cases autant d'espaces et de `-` qu'on veut
    /// (`D6-D5`, `D6 - D5`, `D6D5`).
    fn from_str(s: &str) -> Result<Self, NotationError> {
        let error = || NotationError(s.to_string());
        let text = s.trim().as_bytes();
        let (from, rest) = square(text).ok_or_else(error)?;
        let rest = rest
            .iter()
            .position(|b| !matches!(b, b' ' | b'-'))
            .map_or(&[][..], |i| &rest[i..]);
        let (to, rest) = square(rest).ok_or_else(error)?;
        if rest.is_empty() {
            Ok(Self { from, to })
        } else {
            Err(error())
        }
    }
}

fn square(text: &[u8]) -> Option<(u8, &[u8])> {
    let (&letter, rest) = text.split_first()?;
    let x = letter.checked_sub(b'A').filter(|&x| x < SIZE)?;
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    let row: u8 = std::str::from_utf8(&rest[..digits]).ok()?.parse().ok()?;
    let y = SIZE.checked_sub(row).filter(|_| (1..=SIZE).contains(&row))?;
    Some((y * SIZE + x, &rest[digits..]))
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Hnefatafl;

impl Game for Hnefatafl {
    type State = Board;
    type Move = Move;

    fn descriptor(&self) -> Descriptor {
        let piece = |code: u8, label: &str, style: &str| Piece {
            code,
            label: label.into(),
            style: style.into(),
            side: side_of(code),
        };
        let mut special: Vec<Special> = CORNERS
            .iter()
            .map(|&cell| Special {
                cell: u16::from(cell),
                style: "corner".into(),
            })
            .collect();
        special.push(Special {
            cell: u16::from(THRONE),
            style: "throne".into(),
        });
        Descriptor {
            game: "hnefatafl".into(),
            version: VERSION,
            name: "Hnefatafl".into(),
            rows: SIZE,
            cols: SIZE,
            sides: ByColor {
                red: SideInfo {
                    label: "Attaquants".into(),
                },
                black: SideInfo {
                    label: "Défenseurs".into(),
                },
            },
            initial: Board::initial().cells.to_vec(),
            pieces: vec![
                piece(EMPTY, "case vide", "empty"),
                piece(DEFENDER, "défenseur", "pawn"),
                piece(ATTACKER, "attaquant", "pawn"),
                piece(KING, "roi", "king"),
            ],
            special,
            reasons: BTreeMap::from([
                ("king_captured".into(), "roi pris".into()),
                ("king_escaped".into(), "roi sorti par un coin".into()),
                ("stalemate".into(), "plus aucun coup".into()),
                ("max_plies".into(), format!("{MAX_PLIES} demi-coups")),
            ]),
            max_plies: Some(MAX_PLIES),
        }
    }

    fn initial(&self) -> Board {
        Board::initial()
    }

    fn cells(&self, board: &Board) -> Vec<u8> {
        board.cells.to_vec()
    }

    fn read_cells(&self, cells: &[u8]) -> Option<Board> {
        Board::from_cells(cells)
    }

    fn is_legal(&self, board: &Board, side: Color, mv: Move) -> bool {
        if usize::from(mv.from) >= CELLS || side_of(board.get(mv.from)) != Some(side) {
            return false;
        }
        let mut legal = false;
        board.destinations(mv.from, |to| legal |= to == mv.to);
        legal
    }

    fn legal_moves(&self, board: &Board, side: Color) -> Vec<Move> {
        board.moves(side)
    }

    fn apply(&self, board: &mut Board, _side: Color, mv: Move) -> Vec<Change> {
        let piece = board.get(mv.from);
        let captured = board.play(mv);
        let mut changes = vec![
            Change::new(usize::from(mv.from), EMPTY, Role::From),
            Change::new(usize::from(mv.to), piece, Role::To),
        ];
        changes.extend(
            captured
                .into_iter()
                .map(|c| Change::new(usize::from(c), EMPTY, Role::Effect)),
        );
        changes
    }

    /// Comme le serveur : seule la victoire de celui qui vient de jouer est vérifiée, puis l'adversaire
    /// sans coup fait une nulle. Puis la limite de demi-coups du ladder.
    fn ending(&self, board: &Board, mover: Color) -> Option<Ending> {
        let end = |winner, reason: &str| {
            Some(Ending {
                winner,
                reason: Reason::Game(reason.into()),
            })
        };
        match mover {
            Color::Red if board.king_captured() => return end(Some(Color::Red), "king_captured"),
            Color::Black if board.king_escaped() => return end(Some(Color::Black), "king_escaped"),
            _ => {}
        }
        if !board.has_moves(mover.other()) {
            return end(None, "stalemate");
        }
        (board.plies >= MAX_PLIES).then_some(Ending {
            winner: None,
            reason: Reason::Game("max_plies".into()),
        })
    }

    fn protocol_move(&self, mv: Move) -> String {
        format!("{} - {}", cell_name(mv.from), cell_name(mv.to))
    }

    fn dialect(&self) -> Dialect {
        Dialect::Classic
    }
}

#[cfg(test)]
mod tests;
