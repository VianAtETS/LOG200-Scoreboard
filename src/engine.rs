//! Moteur du niveau 4 : negamax PVS avec table de transposition, et l'évaluation du niveau 3.

use std::cmp::Reverse;
use std::ops::RangeInclusive;
use std::time::Instant;

use rand::Rng;
use rand::seq::SliceRandom;

use crate::board::{Board, CELLS, Cell, Color, Move, SIZE, Square};
use crate::cpu::{self, Choice, Level};
use crate::rules;

const WIN: i32 = 1_000_000;
const WIN_BOUND: i32 = WIN - 1000;
const INF: i32 = WIN + 1;
const MAX_DEPTH: u8 = 64;
const MAX_PLY: usize = MAX_DEPTH as usize + 1;
const CHECK_EVERY: u64 = 1024;
const TABLE_SIZE: usize = 1 << 20;
const STATIC_WEIGHT: i32 = 64;
const PICKED: usize = 3;
const PADDED: usize = SIZE as usize + 6;
const STEPS: [isize; 8] = [-19, -18, -17, -1, 1, 17, 18, 19];

pub fn choose(board: &Board, color: Color, deadline: Instant, rng: &mut impl Rng) -> Option<Choice> {
    let mut moves = rules::legal_moves(board, color);
    moves.shuffle(rng);
    let &first = moves.first()?;
    if moves.len() == 1 {
        return Some(Choice { mv: first, depth: None });
    }
    let mut board = board.clone();
    let mut search = Search::new(&board, color, deadline);
    let mut ordered = Vec::new();
    search.order(&board, color, &moves, None, 0, &mut ordered);
    ordered.sort_unstable_by_key(|&(score, _)| Reverse(score));
    let mut moves: Vec<Move> = ordered.into_iter().map(|(_, mv)| mv).collect();
    let mut best = Choice {
        mv: moves[0],
        depth: Some(0),
    };
    for depth in 1..=MAX_DEPTH {
        search.age_history();
        let found = search.root(&mut board, &mut moves, depth);
        if let Some((mv, _)) = found {
            best.mv = mv;
        }
        if search.timed_out {
            break;
        }
        best.depth = Some(depth);
        if found.is_some_and(|(_, value)| value.abs() > WIN_BOUND) {
            break;
        }
    }
    Some(best)
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
    mv: Option<Move>,
    value: i32,
    depth: u8,
    bound: Bound,
}

struct Search {
    own: Color,
    deadline: Instant,
    nodes: u64,
    timed_out: bool,
    hash: u64,
    table: Vec<Entry>,
    scratch: Vec<Move>,
    buffers: Vec<Vec<(i32, Move)>>,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: Vec<i32>,
    grid: Grid,
}

impl Search {
    fn new(board: &Board, own: Color, deadline: Instant) -> Self {
        Self {
            own,
            deadline,
            nodes: 0,
            timed_out: false,
            hash: cpu::hash(board),
            table: vec![Entry::default(); TABLE_SIZE],
            scratch: Vec::new(),
            buffers: vec![Vec::new(); MAX_PLY],
            killers: [[None; 2]; MAX_PLY],
            history: vec![0; 2 * CELLS * CELLS],
            grid: Grid::new(board),
        }
    }

    /// Ramène le meilleur coup en tête de `moves`.
    fn root(&mut self, board: &mut Board, moves: &mut [Move], depth: u8) -> Option<(Move, i32)> {
        let other = self.own.other();
        let mut alpha = -INF;
        let mut best = None;
        for (index, &mv) in moves.iter().enumerate() {
            self.play(board, mv);
            let value = if index == 0 {
                -self.negamax(board, other, depth - 1, 1, -INF, -alpha)
            } else {
                let value = -self.negamax(board, other, depth - 1, 1, -alpha - 1, -alpha);
                if value > alpha && !self.timed_out {
                    -self.negamax(board, other, depth - 1, 1, -INF, -alpha)
                } else {
                    value
                }
            };
            self.unplay(board, mv);
            if self.timed_out {
                break;
            }
            if value > alpha {
                alpha = value;
                best = Some((index, value));
            }
        }
        let (index, value) = best?;
        moves[..=index].rotate_right(1);
        Some((moves[0], value))
    }

    fn negamax(&mut self, board: &mut Board, side: Color, depth: u8, ply: u8, mut alpha: i32, beta: i32) -> i32 {
        if self.out_of_time() {
            return 0;
        }
        if depth == 0 {
            return self.leaf(board, side, ply);
        }
        let slot = self.slot();
        let entry = self.table[slot];
        let mut table_move = None;
        if entry.key == self.hash {
            table_move = entry.mv;
            let value = from_table(entry.value, ply);
            let usable = match entry.bound {
                Bound::Exact => true,
                Bound::Lower => value >= beta,
                Bound::Upper => value <= alpha,
            };
            if entry.depth >= depth && usable {
                return value;
            }
        }
        if depth == 1 {
            return self.frontier(board, side, ply, alpha, beta, slot);
        }
        rules::generate(board, side, &mut self.scratch);
        let mut moves = std::mem::take(&mut self.buffers[usize::from(ply)]);
        self.order(board, side, &self.scratch, table_move, ply, &mut moves);
        let Some(&(_, mut best_move)) = moves.first() else {
            self.buffers[usize::from(ply)] = moves;
            return loss(ply);
        };
        let original_alpha = alpha;
        let mut best = -INF;
        for index in 0..moves.len() {
            if index < PICKED {
                pick_best(&mut moves[index..]);
            } else if index == PICKED {
                moves[index..].sort_unstable_by_key(|&(score, _)| Reverse(score));
            }
            let mv = moves[index].1;
            self.play(board, mv);
            let value = if index == 0 {
                -self.negamax(board, side.other(), depth - 1, ply + 1, -beta, -alpha)
            } else {
                let value = -self.negamax(board, side.other(), depth - 1, ply + 1, -alpha - 1, -alpha);
                if value > alpha && value < beta && !self.timed_out {
                    -self.negamax(board, side.other(), depth - 1, ply + 1, -beta, -alpha)
                } else {
                    value
                }
            };
            self.unplay(board, mv);
            if self.timed_out {
                break;
            }
            if value > best {
                best = value;
                best_move = mv;
                if value > alpha {
                    alpha = value;
                    if alpha >= beta {
                        self.reward(side, mv, depth, ply);
                        break;
                    }
                }
            }
        }
        self.buffers[usize::from(ply)] = moves;
        if self.timed_out {
            return 0;
        }
        self.store(slot, best_move, best, depth, ply, bound(best, original_alpha, beta));
        best
    }

    /// Parcourt les coups dans l'ordre de `rules::generate` sans en dresser la liste
    fn frontier(&mut self, board: &mut Board, side: Color, ply: u8, alpha: i32, beta: i32, slot: usize) -> i32 {
        let (from, them) = (board.pawn(side), board.pawn(side.other()));
        let mut destinations = Squares::new(from);
        rules::destinations(board, from, |to| destinations.push(to));
        let destinations = destinations.as_slice();
        let mut targets = Squares::new(from);
        let playable = rules::destroy_rings(board.destroyed()).iter().any(|ring| {
            targets = Squares::new(from);
            for target in rules::ring(them, ring.clone()).filter(|&target| target == from || board.is_empty(target)) {
                targets.push(target);
            }
            match targets.as_slice() {
                [] => false,
                &[only] => destinations.iter().any(|&to| to != only),
                _ => !destinations.is_empty(),
            }
        });
        if !playable {
            return loss(ply);
        }
        let destroyed = board.destroyed() + 1;
        let ring = rules::first_ring(destroyed);
        self.grid.open[padded(from)] = true;
        let (mut best, mut best_move) = (-INF, None);
        'search: for &to in destinations {
            let mut moves = targets.as_slice().iter().filter(|&&target| target != to).peekable();
            if moves.peek().is_none() {
                continue;
            }
            self.grid.open[padded(to)] = false;
            let arrival = Arrival::new(&self.grid, to, them, ring.clone());
            self.grid.open[padded(to)] = true;
            for &target in moves {
                let mv = Move {
                    from,
                    to,
                    destroyed: target,
                };
                let value = arrival.value(target, destroyed, ply).unwrap_or_else(|| {
                    board.apply(mv);
                    let value = -self.leaf(board, side.other(), ply + 1);
                    board.undo(mv);
                    value
                });
                if value > best {
                    best = value;
                    best_move = Some(mv);
                    if value >= beta {
                        break 'search;
                    }
                }
            }
        }
        self.grid.open[padded(from)] = false;
        let Some(best_move) = best_move else {
            return loss(ply);
        };
        if best >= beta {
            self.reward(side, best_move, 1, ply);
        }
        self.store(slot, best_move, best, 1, ply, bound(best, alpha, beta));
        best
    }

    fn store(&mut self, slot: usize, mv: Move, value: i32, depth: u8, ply: u8, bound: Bound) {
        self.table[slot] = Entry {
            key: self.hash,
            mv: Some(mv),
            value: to_table(value, ply),
            depth,
            bound,
        };
    }

    fn leaf(&self, board: &Board, side: Color, ply: u8) -> i32 {
        let Some(movable) = rules::playable_mobility(board, side) else {
            return loss(ply);
        };
        let other = rules::mobility(board, side.other());
        let own = self.own;
        let (mine, theirs) = if side == own {
            (movable, other)
        } else {
            (other, movable)
        };
        let value = cpu::heuristic(board, own, mine, theirs, Level::Three);
        if side == own { value } else { -value }
    }

    /// Remplace `out` par `moves` notés, sans les trier.
    fn order(
        &self,
        board: &Board,
        side: Color,
        moves: &[Move],
        table_move: Option<Move>,
        ply: u8,
        out: &mut Vec<(i32, Move)>,
    ) {
        out.clear();
        let grid = &self.grid;
        let mut threatened = [false; PADDED * PADDED];
        let them = padded(board.pawn(side.other()));
        for step in STEPS {
            let mut current = them;
            for _ in 0..grid.ray(them, step) {
                current = current.wrapping_add_signed(step);
                threatened[current] = true;
            }
        }
        let mut reach = [u8::MAX; CELLS];
        let killers = self.killers[usize::from(ply)];
        for &mv in moves {
            let score = if Some(mv) == table_move {
                i32::MAX
            } else if Some(mv) == killers[0] {
                i32::MAX - 1
            } else if Some(mv) == killers[1] {
                i32::MAX - 2
            } else {
                let to = mv.to.index();
                if reach[to] == u8::MAX {
                    let start = padded(mv.to);
                    reach[to] = STEPS.into_iter().map(|step| grid.ray(start, step)).sum();
                }
                let heuristic = i32::from(reach[to]) + 8 * i32::from(threatened[padded(mv.destroyed)]);
                self.history[history_index(side, mv)] + STATIC_WEIGHT * heuristic
            };
            out.push((score, mv));
        }
    }

    fn reward(&mut self, side: Color, mv: Move, depth: u8, ply: u8) {
        let killers = &mut self.killers[usize::from(ply)];
        if killers[0] != Some(mv) {
            killers[1] = killers[0];
            killers[0] = Some(mv);
        }
        let bonus = i32::from(depth) * i32::from(depth);
        let entry = &mut self.history[history_index(side, mv)];
        *entry = entry.saturating_add(bonus);
    }

    fn age_history(&mut self) {
        for value in &mut self.history {
            *value /= 2;
        }
    }

    fn out_of_time(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes.is_multiple_of(CHECK_EVERY) && Instant::now() > self.deadline {
            self.timed_out = true;
        }
        self.timed_out
    }

    fn play(&mut self, board: &mut Board, mv: Move) {
        self.hash ^= cpu::move_key(board, mv);
        board.apply(mv);
        self.grid.open[padded(mv.from)] = true;
        self.grid.open[padded(mv.to)] = false;
        self.grid.open[padded(mv.destroyed)] = false;
    }

    fn unplay(&mut self, board: &mut Board, mv: Move) {
        board.undo(mv);
        self.hash ^= cpu::move_key(board, mv);
        self.grid.open[padded(mv.destroyed)] = true;
        self.grid.open[padded(mv.to)] = true;
        self.grid.open[padded(mv.from)] = false;
    }

    #[expect(clippy::cast_possible_truncation, reason = "les 20 bits bas du hash")]
    const fn slot(&self) -> usize {
        self.hash as usize & (TABLE_SIZE - 1)
    }
}

/// Le plateau entouré de 3 rangées de cases fermées pour suivre les rayons sans tester les bords
struct Grid {
    open: [bool; PADDED * PADDED],
}

impl Grid {
    fn new(board: &Board) -> Self {
        let mut open = [false; PADDED * PADDED];
        for (square, &cell) in (0..CELLS).filter_map(Square::from_index).zip(board.cells()) {
            open[padded(square)] = cell == Cell::Empty;
        }
        Self { open }
    }

    #[cfg(test)]
    fn matches(&self, board: &Board) -> bool {
        self.open == Self::new(board).open
    }

    fn ray(&self, start: usize, step: isize) -> u8 {
        let mut length = 0;
        let mut current = start;
        while length < 3 {
            current = current.wrapping_add_signed(step);
            if !self.open[current] {
                break;
            }
            length += 1;
        }
        length
    }
}

fn padded(square: Square) -> usize {
    (usize::from(square.y()) + 3) * PADDED + usize::from(square.x()) + 3
}

/// Jusqu'à 24 cases sans allocation.
struct Squares {
    squares: [Square; 24],
    len: usize,
}

impl Squares {
    const fn new(filler: Square) -> Self {
        Self {
            squares: [filler; 24],
            len: 0,
        }
    }

    fn push(&mut self, square: Square) {
        self.squares[self.len] = square;
        self.len += 1;
    }

    fn as_slice(&self) -> &[Square] {
        &self.squares[..self.len]
    }
}

/// Le plateau après le déplacement vers `to`, avant la destruction.
struct Arrival {
    to: Square,
    them: Square,
    /// Celui qui joue, puis l'adversaire.
    mobility: [i32; 2],
    rays: [[u8; 8]; 2],
    free: [i32; 2],
    ring: RangeInclusive<i8>,
    ring_count: i32,
    edges: i32,
}

impl Arrival {
    fn new(grid: &Grid, to: Square, them: Square, ring: RangeInclusive<i8>) -> Self {
        let centers = [padded(to), padded(them)];
        let rays = centers.map(|center| STEPS.map(|step| grid.ray(center, step)));
        let mobility = rays.map(|lengths| lengths.iter().map(|&length| i32::from(length)).sum());
        let free = centers.map(|center| {
            STEPS
                .into_iter()
                .map(|step| i32::from(grid.open[center.wrapping_add_signed(step)]))
                .sum()
        });
        let mut ring_count = 0;
        for step in STEPS {
            for k in ring.clone() {
                let square = centers[0].wrapping_add_signed(step * isize::from(k));
                ring_count += i32::from(square == centers[1] || grid.open[square]);
            }
        }
        Self {
            to,
            them,
            mobility,
            rays,
            free,
            ring,
            ring_count,
            edges: cpu::edge_distance(to) - cpu::edge_distance(them),
        }
    }

    /// `None` quand il faut jouer le coup pour savoir si l'adversaire peut encore jouer.
    fn value(&self, square: Square, destroyed: u32, ply: u8) -> Option<i32> {
        let theirs = self.mobility[1] - cut(alignment(self.them, square), self.rays[1]);
        if theirs == 0 {
            return Some(-loss(ply + 1));
        }
        let line = alignment(self.to, square);
        let in_ring = line.is_some_and(|(_, distance)| self.ring.contains(&distance));
        if self.ring_count - i32::from(in_ring) < 2 {
            return None;
        }
        let mine = self.mobility[0] - cut(line, self.rays[0]);
        let free = (self.free[0] - adjacent(square, self.to)) - (self.free[1] - adjacent(square, self.them));
        Some(cpu::level_three(destroyed, self.edges, mine - theirs, free))
    }
}

/// Les destinations perdues si la case alignée ainsi (voir `alignment`) est détruite.
fn cut(line: Option<(usize, i8)>, rays: [u8; 8]) -> i32 {
    line.map_or(0, |(direction, distance)| {
        (i32::from(rays[direction]) - i32::from(distance) + 1).max(0)
    })
}

fn alignment(center: Square, square: Square) -> Option<(usize, i8)> {
    let dx = square.x().cast_signed() - center.x().cast_signed();
    let dy = square.y().cast_signed() - center.y().cast_signed();
    let distance = dx.abs().max(dy.abs());
    if distance == 0 || (dx != 0 && dy != 0 && dx.abs() != dy.abs()) {
        return None;
    }
    let code = usize::from((dx.signum() + 1).unsigned_abs() + 3 * (dy.signum() + 1).unsigned_abs());
    Some((code - usize::from(code > 4), distance))
}

fn adjacent(a: Square, b: Square) -> i32 {
    i32::from(a.x().abs_diff(b.x()) <= 1 && a.y().abs_diff(b.y()) <= 1)
}

const fn bound(value: i32, alpha: i32, beta: i32) -> Bound {
    if value <= alpha {
        Bound::Upper
    } else if value >= beta {
        Bound::Lower
    } else {
        Bound::Exact
    }
}

fn pick_best(moves: &mut [(i32, Move)]) {
    let best = (0..moves.len()).max_by_key(|&index| moves[index].0).unwrap_or(0);
    moves.swap(0, best);
}

fn loss(ply: u8) -> i32 {
    i32::from(ply) - WIN
}

/// Dans la table, les victoires se comptent depuis le nœud.
fn to_table(value: i32, ply: u8) -> i32 {
    match value {
        v if v > WIN_BOUND => v + i32::from(ply),
        v if v < -WIN_BOUND => v - i32::from(ply),
        v => v,
    }
}

fn from_table(value: i32, ply: u8) -> i32 {
    match value {
        v if v > WIN_BOUND => v - i32::from(ply),
        v if v < -WIN_BOUND => v + i32::from(ply),
        v => v,
    }
}

fn history_index(side: Color, mv: Move) -> usize {
    (usize::from(side == Color::Black) * CELLS + mv.to.index()) * CELLS + mv.destroyed.index()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::board::DIRS;
    use crate::board::test_support::diagram;
    use crate::rules::test_support::positions;

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    fn brute(search: &Search, board: &mut Board, side: Color, depth: u8, ply: u8) -> i32 {
        if depth == 0 {
            return search.leaf(board, side, ply);
        }
        rules::legal_moves(board, side)
            .into_iter()
            .map(|mv| {
                board.apply(mv);
                let value = -brute(search, board, side.other(), depth - 1, ply + 1);
                board.undo(mv);
                value
            })
            .max()
            .unwrap_or_else(|| loss(ply))
    }

    fn assert_root_matches_brute_force(board: &Board, color: Color, depth: u8) {
        let mut scratch = board.clone();
        let mut search = Search::new(&scratch, color, far());
        let start = search.hash;
        let mut moves = rules::legal_moves(board, color);
        let (_, value) = search.root(&mut scratch, &mut moves, depth).unwrap();
        let expected = brute(&search, &mut board.clone(), color, depth, 0);
        assert_eq!(value, expected, "profondeur {depth}\n{board}");
        assert_eq!(&scratch, board);
        assert_eq!(search.hash, start);
        assert!(search.grid.matches(board));
    }

    #[test]
    fn root_value_matches_brute_force() {
        for (board, color) in positions(11, 6, 30) {
            for depth in 1..=2 {
                assert_root_matches_brute_force(&board, color, depth);
            }
        }
        for (board, color) in positions(5, 4, 12) {
            assert_root_matches_brute_force(&board, color, 3);
        }
    }

    #[test]
    fn arrival_matches_playing_the_move() {
        let mut boards: Vec<(Board, Color)> = (0..20)
            .filter_map(|game| {
                let opening = rules::random_opening(4 + 3 * game, &mut StdRng::seed_from_u64(game as u64));
                let replay = rules::Replay::from_moves(opening).ok()?;
                replay.winner.is_none().then_some((replay.board, replay.turn))
            })
            .collect();
        boards.extend(positions(4, 30, 60));
        let (mut fast, mut slow) = (0, 0);
        for (board, color) in boards {
            let search = Search::new(&board, color, far());
            let them = board.pawn(color.other());
            let destroyed = board.destroyed() + 1;
            for mv in rules::legal_moves(&board, color) {
                let mut grid = Grid::new(&board);
                grid.open[padded(mv.from)] = true;
                grid.open[padded(mv.to)] = false;
                let arrival = Arrival::new(&grid, mv.to, them, rules::first_ring(destroyed));
                let mut after = board.clone();
                after.apply(mv);
                let expected = -search.leaf(&after, color.other(), 1);
                if let Some(value) = arrival.value(mv.destroyed, destroyed, 0) {
                    assert_eq!(value, expected, "{mv}\n{board}");
                    fast += 1;
                } else {
                    slow += 1;
                }
            }
        }
        assert!(fast > 10 * slow, "{fast} rapides, {slow} lents");
    }

    #[test]
    fn steps_follow_dirs() {
        let center = Square::new(5, 5).unwrap();
        for ((dx, dy), step) in DIRS.into_iter().zip(STEPS) {
            let neighbour = center.offset(dx, dy).unwrap();
            assert_eq!(padded(center).wrapping_add_signed(step), padded(neighbour));
            assert_eq!(
                alignment(center, neighbour).map(|(direction, _)| STEPS[direction]),
                Some(step)
            );
        }
    }

    #[test]
    fn table_keeps_win_distances() {
        let roundtrip = |value, ply| from_table(to_table(value, ply), ply);
        assert_eq!(to_table(WIN - 7, 3), WIN - 4, "victoire à 4 demi-coups du nœud");
        assert_eq!(to_table(loss(9), 4), loss(5));
        assert_eq!(roundtrip(WIN - 7, 3), WIN - 7);
        assert_eq!(roundtrip(loss(9), 4), loss(9));
        assert_eq!(to_table(12, 5), 12);
    }

    #[test]
    fn takes_an_immediate_win() {
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
        let choice = choose(&board, Color::Red, far(), &mut StdRng::seed_from_u64(1)).unwrap();
        let mut after = board;
        after.apply(choice.mv);
        assert_eq!(
            rules::winner_after(&after, Color::Red),
            Some(Color::Red),
            "{}",
            choice.mv
        );
        assert_eq!(choice.depth, Some(1), "la victoire arrête l'approfondissement");
    }

    #[test]
    fn stops_near_the_deadline() {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(100);
        let choice = choose(&Board::initial(), Color::Red, deadline, &mut StdRng::seed_from_u64(2)).unwrap();
        assert!(rules::is_legal(&Board::initial(), Color::Red, choice.mv));
        assert!(start.elapsed() < Duration::from_millis(400), "{:?}", start.elapsed());
    }

    /// `cargo test --release --lib engine::tests::bench -- --ignored --nocapture`
    #[test]
    #[ignore = "mesure de vitesse, à lancer en release"]
    fn bench() {
        let start = Instant::now();
        let mut nodes = 0;
        for game in 0..10 {
            for plies in [4, 14, 24] {
                let opening = rules::random_opening(plies, &mut StdRng::seed_from_u64(game));
                let Ok(replay) = rules::Replay::from_moves(opening) else {
                    continue;
                };
                if replay.winner.is_some() {
                    continue;
                }
                let mut board = replay.board;
                let mut search = Search::new(&board, replay.turn, far());
                let mut moves = rules::legal_moves(&board, replay.turn);
                for depth in 1..=5 {
                    search.age_history();
                    search.root(&mut board, &mut moves, depth);
                }
                nodes += search.nodes;
            }
        }
        let elapsed = start.elapsed();
        #[expect(clippy::cast_precision_loss, reason = "débit affiché")]
        let rate = nodes as f64 / elapsed.as_secs_f64() / 1000.0;
        println!("{nodes} nœuds en {elapsed:.2?}, {rate:.0} k nœuds/s");
    }
}
