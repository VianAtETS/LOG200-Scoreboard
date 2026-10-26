//! Réplique du joueur CPU du serveur officiel (`BoardGame.exe`), niveaux 1 à 3. Ses bizarreries
//! sont gardées exprès : une victoire lointaine vaut plus qu'une proche, et un joueur bloqué termine
//! la recherche même quand ce n'est pas à lui de jouer.

use std::time::Instant;

use rand::Rng;
use rand::seq::SliceRandom;

use crate::board::{Board, CELLS, Cell, Color, DIRS, Move, SIZE, Square};
use crate::rules;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    One,
    Two,
    Three,
}

impl Level {
    const fn max_depth(self) -> u8 {
        match self {
            Self::One => 2,
            Self::Two | Self::Three => 30,
        }
    }
}

const WIN: i32 = 9_000_000;
const WIN_FOUND: i32 = WIN - 101;
const INF: i32 = 1_000_000_000;
const CHECK_EVERY: u64 = 1000;
const TABLE_SIZE: usize = 1 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice {
    pub mv: Move,
    /// Dernière profondeur terminée. `None` sans recherche, `Some(0)` si le temps a manqué : le coup
    /// est alors tiré au hasard.
    pub depth: Option<u8>,
}

pub fn choose(board: &Board, color: Color, level: Level, deadline: Instant, rng: &mut impl Rng) -> Option<Choice> {
    let mut moves = rules::legal_moves(board, color);
    moves.shuffle(rng);
    let &first = moves.first()?;
    if moves.len() == 1 {
        return Some(Choice { mv: first, depth: None });
    }
    let mut board = board.clone();
    let mut search = Search::new(&board, color, level, Some(deadline));
    let mut best = Choice {
        mv: first,
        depth: Some(0),
    };
    for depth in 2..=level.max_depth() {
        let Some((mv, value)) = search.root(&mut board, &moves, depth) else {
            break;
        };
        best = Choice { mv, depth: Some(depth) };
        if value > WIN_FOUND {
            break;
        }
    }
    Some(best)
}

/// Coups que le niveau 1 peut jouer : il s'arrête au premier coup gagnant de son mélange, sinon il
/// garde le premier de valeur maximale.
pub fn accepted_moves(board: &Board, color: Color) -> Vec<Move> {
    let moves = rules::legal_moves(board, color);
    if moves.len() < 2 {
        return moves;
    }
    let depth = Level::One.max_depth();
    let mut board = board.clone();
    let mut search = Search::new(&board, color, Level::One, None);
    let values: Vec<i32> = moves
        .iter()
        .map(|&mv| {
            board.apply(mv);
            let value = search.minimax(&mut board, false, -INF, INF, depth);
            board.undo(mv);
            value
        })
        .collect();
    let max = values.iter().copied().max().unwrap_or(-INF);
    let accepted = |value: i32| {
        if max > WIN_FOUND {
            value > WIN_FOUND
        } else {
            value == max
        }
    };
    moves
        .into_iter()
        .zip(values)
        .filter(|&(_, value)| accepted(value))
        .map(|(mv, _)| mv)
        .collect()
}

#[derive(Clone, Copy, Default)]
enum Bound {
    #[default]
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Default)]
struct Entry {
    key: u64,
    value: i32,
    depth: u8,
    bound: Bound,
}

struct Search {
    own: Color,
    level: Level,
    deadline: Option<Instant>,
    nodes: u64,
    timed_out: bool,
    buffers: Vec<Vec<Move>>,
    hash: u64,
    table: Vec<Entry>,
}

impl Search {
    fn new(board: &Board, own: Color, level: Level, deadline: Option<Instant>) -> Self {
        let hash = (0..CELLS)
            .filter_map(Square::from_index)
            .fold(0, |hash, square| hash ^ zobrist(board.get(square), square));
        Self {
            own,
            level,
            deadline,
            nodes: 0,
            timed_out: false,
            buffers: vec![Vec::new(); usize::from(level.max_depth()) + 1],
            hash,
            table: if level == Level::Three {
                vec![Entry::default(); TABLE_SIZE]
            } else {
                Vec::new()
            },
        }
    }

    fn root(&mut self, board: &mut Board, moves: &[Move], depth: u8) -> Option<(Move, i32)> {
        let mut best = None;
        for &mv in moves {
            let alpha = best.map_or(-INF, |(_, value)| value);
            self.play(board, mv);
            let value = if self.level == Level::Three {
                -self.negamax(board, false, -INF, -alpha, depth)
            } else {
                self.minimax(board, false, alpha, INF, depth)
            };
            self.unplay(board, mv);
            if self.timed_out {
                return None;
            }
            if value > alpha {
                best = Some((mv, value));
            }
            if value > WIN_FOUND {
                break;
            }
        }
        best
    }

    fn out_of_time(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes.is_multiple_of(CHECK_EVERY) && self.deadline.is_some_and(|deadline| Instant::now() > deadline) {
            self.timed_out = true;
        }
        self.timed_out
    }

    fn is_leaf(board: &Board, depth: u8) -> bool {
        depth == 0 || !rules::has_moves(board, Color::Red) || !rules::has_moves(board, Color::Black)
    }

    fn player(&self, own_turn: bool) -> Color {
        if own_turn { self.own } else { self.own.other() }
    }

    fn play(&mut self, board: &mut Board, mv: Move) {
        self.hash ^= move_key(board, mv);
        board.apply(mv);
    }

    fn unplay(&mut self, board: &mut Board, mv: Move) {
        board.undo(mv);
        self.hash ^= move_key(board, mv);
    }

    /// Bornes dures : un nœud max rend au moins `alpha`, un nœud min au plus `beta`.
    fn minimax(&mut self, board: &mut Board, own_turn: bool, mut alpha: i32, mut beta: i32, depth: u8) -> i32 {
        if self.out_of_time() {
            return 0;
        }
        if Self::is_leaf(board, depth) {
            return eval(board, self.own, depth, self.level);
        }
        let mut moves = std::mem::take(&mut self.buffers[usize::from(depth)]);
        rules::generate(board, self.player(own_turn), &mut moves);
        for &mv in &moves {
            self.play(board, mv);
            let value = self.minimax(board, !own_turn, alpha, beta, depth - 1);
            self.unplay(board, mv);
            if own_turn {
                alpha = alpha.max(value);
            } else {
                beta = beta.min(value);
            }
            if beta <= alpha || self.timed_out {
                break;
            }
        }
        self.buffers[usize::from(depth)] = moves;
        if own_turn { alpha } else { beta }
    }

    /// La table est consultée avant même la profondeur 0, comme dans le CPU.
    fn negamax(&mut self, board: &mut Board, own_turn: bool, alpha: i32, mut beta: i32, depth: u8) -> i32 {
        if self.out_of_time() {
            return 0;
        }
        let mut floor = alpha;
        let slot = self.slot();
        let entry = self.table[slot];
        if entry.key == self.hash && entry.key != 0 && depth <= entry.depth {
            match entry.bound {
                Bound::Exact => return entry.value,
                Bound::Lower => floor = floor.max(entry.value),
                Bound::Upper => beta = beta.min(entry.value),
            }
            if beta <= floor {
                return entry.value;
            }
        }
        let sign = if own_turn { 1 } else { -1 };
        if Self::is_leaf(board, depth) {
            return sign * eval(board, self.own, depth, self.level);
        }
        let mut moves = std::mem::take(&mut self.buffers[usize::from(depth)]);
        rules::generate(board, self.player(own_turn), &mut moves);
        let mut best = -INF;
        for &mv in &moves {
            self.play(board, mv);
            let value = -self.negamax(board, !own_turn, -beta, -floor, depth - 1);
            self.unplay(board, mv);
            best = best.max(value);
            floor = floor.max(value);
            if beta <= floor || self.timed_out {
                break;
            }
        }
        self.buffers[usize::from(depth)] = moves;
        let bound = if best <= alpha {
            Bound::Upper
        } else if best < beta {
            Bound::Exact
        } else {
            Bound::Lower
        };
        self.table[slot] = Entry {
            key: self.hash,
            value: best,
            depth,
            bound,
        };
        best
    }

    #[expect(clippy::cast_possible_truncation, reason = "les 20 bits bas du hash")]
    const fn slot(&self) -> usize {
        self.hash as usize & (TABLE_SIZE - 1)
    }
}

/// La victoire se lit sur la mobilité, pas sur les coups complets.
fn eval(board: &Board, own: Color, depth: u8, level: Level) -> i32 {
    let depth = i32::from(depth);
    let mine = rules::mobility(board, own);
    let theirs = rules::mobility(board, own.other());
    if mine == 0 {
        depth - WIN
    } else if theirs == 0 {
        WIN - depth
    } else if level == Level::Three && board.destroyed() < 20 {
        let (me, them) = (board.pawn(own), board.pawn(own.other()));
        3 * (edge_distance(me) - edge_distance(them))
            + 2 * (mine - theirs)
            + 4 * (free_neighbours(board, me) - free_neighbours(board, them))
    } else {
        5 * (mine - theirs)
    }
}

fn edge_distance(square: Square) -> i32 {
    let last = SIZE - 1;
    i32::from(square.x().min(square.y()).min(last - square.x()).min(last - square.y()))
}

fn free_neighbours(board: &Board, square: Square) -> i32 {
    DIRS.into_iter()
        .map(|(dx, dy)| i32::from(square.offset(dx, dy).is_some_and(|n| board.is_empty(n))))
        .sum()
}

fn move_key(board: &Board, mv: Move) -> u64 {
    let pawn = board.get(mv.from);
    zobrist(pawn, mv.from) ^ zobrist(pawn, mv.to) ^ zobrist(Cell::Destroyed, mv.destroyed)
}

fn zobrist(cell: Cell, square: Square) -> u64 {
    if cell == Cell::Empty {
        0
    } else {
        ZOBRIST[usize::from(cell.code())][square.index()]
    }
}

const ZOBRIST: [[u64; CELLS]; 6] = {
    let mut keys = [[0; CELLS]; 6];
    let mut state: u64 = 0;
    let mut code = 0;
    while code < 6 {
        let mut index = 0;
        while index < CELLS {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            keys[code][index] = z ^ (z >> 31);
            index += 1;
        }
        code += 1;
    }
    keys
};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rand::seq::IndexedRandom;

    use super::*;
    use crate::board::test_support::diagram;

    fn brute(board: &mut Board, own: Color, own_turn: bool, depth: u8, level: Level) -> i32 {
        let stuck = |board: &Board, color| rules::legal_moves(board, color).is_empty();
        if depth == 0 || stuck(board, Color::Red) || stuck(board, Color::Black) {
            return eval(board, own, depth, level);
        }
        let mover = if own_turn { own } else { own.other() };
        let values = rules::legal_moves(board, mover).into_iter().map(|mv| {
            board.apply(mv);
            let value = brute(board, own, !own_turn, depth - 1, level);
            board.undo(mv);
            value
        });
        if own_turn { values.max() } else { values.min() }.unwrap()
    }

    fn brute_values(board: &Board, color: Color, depth: u8, level: Level) -> Vec<(Move, i32)> {
        let mut board = board.clone();
        rules::legal_moves(&board, color)
            .into_iter()
            .map(|mv| {
                board.apply(mv);
                let value = brute(&mut board, color, false, depth, level);
                board.undo(mv);
                (mv, value)
            })
            .collect()
    }

    fn positions(seed: u64, count: usize, max_moves: usize) -> Vec<(Board, Color)> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut positions = Vec::new();
        while positions.len() < count {
            let mut board = Board::initial();
            let mut turn = Color::Red;
            while let Some(&mv) = rules::legal_moves(&board, turn).choose(&mut rng) {
                board.apply(mv);
                turn = turn.other();
                let moves = rules::legal_moves(&board, turn).len();
                if (2..=max_moves).contains(&moves) && rules::has_moves(&board, turn.other()) {
                    positions.push((board, turn));
                    break;
                }
            }
        }
        positions
    }

    #[test]
    fn accepted_moves_match_brute_force_and_choose_picks_one() {
        let mut rng = StdRng::seed_from_u64(1);
        for (board, color) in positions(11, 6, 30) {
            let values = brute_values(&board, color, 2, Level::One);
            let max = values.iter().map(|&(_, v)| v).max().unwrap();
            let expected: Vec<Move> = values
                .iter()
                .filter(|&&(_, v)| if max > WIN_FOUND { v > WIN_FOUND } else { v == max })
                .map(|&(mv, _)| mv)
                .collect();
            assert_eq!(accepted_moves(&board, color), expected, "\n{board}");

            let deadline = Instant::now() + Duration::from_secs(60);
            let choice = choose(&board, color, Level::One, deadline, &mut rng).unwrap();
            assert!(
                expected.contains(&choice.mv),
                "{} hors de {expected:?}\n{board}",
                choice.mv
            );
            assert_eq!(choice.depth, Some(2));
        }
    }

    fn assert_root_matches_brute_force(board: &Board, color: Color, depth: u8, level: Level) {
        let values = brute_values(board, color, depth, level);
        let max = values.iter().map(|&(_, v)| v).max().unwrap();
        let expected = values
            .iter()
            .find(|&&(_, v)| v > WIN_FOUND)
            .or_else(|| values.iter().find(|&&(_, v)| v == max))
            .copied();
        let moves: Vec<Move> = values.iter().map(|&(mv, _)| mv).collect();
        let mut scratch = board.clone();
        let mut search = Search::new(&scratch, color, level, None);
        assert_eq!(
            search.root(&mut scratch, &moves, depth),
            expected,
            "{level:?}, profondeur {depth}\n{board}"
        );
        assert_eq!(&scratch, board);
    }

    #[test]
    fn every_level_matches_brute_force_in_endgames() {
        for (board, color) in positions(11, 6, 30) {
            for level in [Level::One, Level::Two, Level::Three] {
                assert_root_matches_brute_force(&board, color, 2, level);
            }
        }
    }

    #[test]
    fn level_three_table_keeps_exact_values() {
        // À la profondeur 3, deux ordres de destruction mènent à la même position.
        for (board, color) in positions(5, 4, 12) {
            assert_root_matches_brute_force(&board, color, 3, Level::Three);
        }
    }

    #[test]
    fn level_three_matches_brute_force_before_twenty_destroyed() {
        for (board, color) in positions(4, 3, 200).into_iter().filter(|(b, _)| b.destroyed() < 20) {
            assert_root_matches_brute_force(&board, color, 1, Level::Three);
        }
    }

    #[test]
    fn level_three_counts_edges_and_free_neighbours() {
        // Rouge en A1 : bord 0, mobilité 9, 3 voisins libres. Noir en F6 : bord 5, mobilité 24, 8.
        let board = diagram(
            "............
             ............
             ............
             ............
             ............
             ............
             .....B......
             ............
             ............
             ............
             ............
             R...........",
        );
        assert_eq!(eval(&board, Color::Red, 0, Level::Three), 3 * -5 + 2 * -15 + 4 * -5);
        assert_eq!(eval(&board, Color::Black, 0, Level::Three), 65);
        assert_eq!(eval(&board, Color::Red, 0, Level::One), 5 * -15);
    }

    #[test]
    fn hash_follows_the_board() {
        let (mut board, color) = positions(7, 1, 30).remove(0);
        let mut search = Search::new(&board, color, Level::Three, None);
        let start = search.hash;
        let mv = rules::legal_moves(&board, color)[0];
        search.play(&mut board, mv);
        assert_eq!(search.hash, Search::new(&board, color, Level::One, None).hash);
        search.unplay(&mut board, mv);
        assert_eq!(search.hash, start);
    }

    #[test]
    fn takes_an_immediate_win() {
        // Noir en A12 n'a qu'une sortie (B11) : rouge gagne en la détruisant.
        let board = diagram(
            "B#..........
             #...........
             ............
             ............
             ............
             ......R.....
             ############
             ############
             ############
             ............
             ............
             ............",
        );
        for level in [Level::One, Level::Two, Level::Three] {
            let deadline = Instant::now() + Duration::from_secs(60);
            let choice = choose(&board, Color::Red, level, deadline, &mut StdRng::seed_from_u64(1)).unwrap();
            let mut after = board.clone();
            after.apply(choice.mv);
            assert_eq!(
                rules::winner_after(&after, Color::Red),
                Some(Color::Red),
                "{level:?} : {}",
                choice.mv
            );
            assert_eq!(choice.depth, Some(2), "la victoire arrête l'approfondissement");
        }
    }

    #[test]
    fn out_of_time_plays_the_first_shuffled_move() {
        let board = Board::initial();
        for level in [Level::One, Level::Three] {
            let choice = choose(&board, Color::Red, level, Instant::now(), &mut StdRng::seed_from_u64(5)).unwrap();
            let mut moves = rules::legal_moves(&board, Color::Red);
            moves.shuffle(&mut StdRng::seed_from_u64(5));
            assert_eq!(
                choice,
                Choice {
                    mv: moves[0],
                    depth: Some(0)
                },
                "{level:?}"
            );
        }
    }

    #[test]
    fn stops_near_the_deadline() {
        for level in [Level::Two, Level::Three] {
            let start = Instant::now();
            let deadline = start + Duration::from_millis(100);
            let choice = choose(
                &Board::initial(),
                Color::Red,
                level,
                deadline,
                &mut StdRng::seed_from_u64(2),
            );
            assert!(choice.is_some());
            assert!(
                start.elapsed() < Duration::from_millis(400),
                "{level:?} : {:?}",
                start.elapsed()
            );
        }
    }

    #[test]
    fn forced_move_is_played_without_search() {
        // Rouge en A1 n'a qu'une destination (B1) ; à distance 3 de noir (L12), il ne reste que I9.
        let board = diagram(
            "........#..B
             ............
             ............
             ...........#
             ............
             ............
             ............
             ............
             ............
             ............
             ##..........
             R.#.........",
        );
        let moves = rules::legal_moves(&board, Color::Red);
        assert_eq!(moves.len(), 1);
        let deadline = Instant::now() + Duration::from_secs(60);
        let choice = choose(&board, Color::Red, Level::One, deadline, &mut StdRng::seed_from_u64(1));
        assert_eq!(
            choice,
            Some(Choice {
                mv: moves[0],
                depth: None
            })
        );
    }
}
