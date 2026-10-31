//! Moteurs des niveaux 4 et 5 : negamax PVS avec table de transposition. Le niveau 4 évalue comme
//! le niveau 3 ; le niveau 5 compte les cases que chaque pion atteint en un et en deux coups.

use std::cmp::Reverse;
use std::ops::RangeInclusive;
use std::panic::resume_unwind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

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
const VALUE_OFFSET: i32 = 1 << 21;
/// Valeur d'une case atteinte en un coup, et en deux, pour `Eval::Territory`.
const NEAR_WEIGHT: i32 = 4;
const FAR_WEIGHT: i32 = 1;
/// Coups tardifs cherchés deux demi-coups moins profond, pour garder la parité des feuilles.
const LMR_LATE: usize = 8;
const LMR_DEPTH: u8 = 3;
const STATIC_WEIGHT: i32 = 64;
const PICKED: usize = 3;
// TEMP expériences : N5_STEP, N5_LMR, N5_NEAR, N5_FAR.
static KNOBS: std::sync::LazyLock<[i32; 4]> = std::sync::LazyLock::new(|| {
    let k = |n: &str, d| std::env::var(n).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    [k("N5_STEP", 2), k("N5_LMR", 2), k("N5_NEAR", 4), k("N5_FAR", 1)]
});
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn knob_u8(i: usize) -> u8 { KNOBS[i] as u8 }
const PADDED: usize = SIZE as usize + 6;
const STEPS: [isize; 8] = [-19, -18, -17, -1, 1, 17, 18, 19];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eval {
    /// L'évaluation du niveau 3 (niveau 4).
    LevelThree,
    /// Cases que chaque pion atteint en un et en deux coups (niveau 5).
    Territory,
}

/// Sans mémoire : chaque coup repart d'une table vide.
pub fn choose(board: &Board, color: Color, eval: Eval, deadline: Instant, rng: &mut impl Rng) -> Option<Choice> {
    Engine::new(eval, 1).choose(board, color, deadline, rng)
}

/// Garde la table de transposition et l'historique d'un coup à l'autre de la même partie, et
/// cherche avec plusieurs fils qui se partagent la table. Les valeurs de `Eval::Territory` ne
/// dépendent que du joueur au trait : ce que `ponder` cherche pour l'adversaire sert ensuite.
pub struct Engine {
    eval: Eval,
    threads: usize,
    table: Arc<Table>,
    stop: Arc<AtomicBool>,
    searches: Vec<Search>,
    pondering: Vec<JoinHandle<Search>>,
}

impl Engine {
    pub fn new(eval: Eval, threads: usize) -> Self {
        Self {
            eval,
            threads: threads.max(1),
            table: Arc::new(Table::new(eval == Eval::Territory)),
            stop: Arc::default(),
            searches: Vec::new(),
            pondering: Vec::new(),
        }
    }

    /// Le coup de la plus grande profondeur terminée, tous fils confondus.
    pub fn choose(&mut self, board: &Board, color: Color, deadline: Instant, rng: &mut impl Rng) -> Option<Choice> {
        self.stop_pondering();
        self.prepare(board, color, deadline);
        let seeds: Vec<u64> = (1..self.threads).map(|_| rng.next_u64()).collect();
        let stop = &*self.stop;
        let (main, helpers) = self.searches.split_first_mut()?;
        thread::scope(|scope| {
            let helpers: Vec<_> = helpers
                .iter_mut()
                .zip(seeds)
                .map(|(search, seed)| {
                    scope.spawn(move || search.choose(board, color, 3, &mut StdRng::seed_from_u64(seed)))
                })
                .collect();
            let main = main.choose(board, color, 1, rng);
            stop.store(true, Ordering::Relaxed);
            helpers
                .into_iter()
                .filter_map(|helper| helper.join().unwrap_or_else(|panic| resume_unwind(panic)))
                .fold(main, |best, other| match best {
                    Some(best) if best.depth >= other.depth => Some(best),
                    _ => Some(other),
                })
        })
    }

    /// Cherche pour `side`, l'adversaire au trait, jusqu'au prochain `choose`. Aux profondeurs
    /// paires : nos positions, ses enfants, gardent ainsi des feuilles juste après notre coup, comme
    /// dans nos propres recherches.
    pub fn ponder(&mut self, board: &Board, side: Color) {
        self.stop_pondering();
        self.prepare(board, side, Instant::now() + Duration::from_secs(3600));
        for (index, mut search) in self.searches.drain(..).enumerate() {
            let board = board.clone();
            let first = if knob_u8(0) == 1 { if index == 0 { 1 } else { 3 } } else if index == 0 { 2 } else { 4 };
            self.pondering.push(thread::spawn(move || {
                search.choose(&board, side, first, &mut StdRng::seed_from_u64(index as u64));
                search
            }));
        }
    }

    fn prepare(&mut self, board: &Board, color: Color, deadline: Instant) {
        self.stop.store(false, Ordering::Relaxed);
        while self.searches.len() < self.threads {
            let (table, stop) = (Arc::clone(&self.table), Arc::clone(&self.stop));
            self.searches
                .push(Search::shared(board, color, self.eval, deadline, table, stop));
        }
        for search in &mut self.searches {
            search.reset(board, color, deadline);
        }
    }

    fn stop_pondering(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.pondering.drain(..) {
            self.searches
                .push(handle.join().unwrap_or_else(|panic| resume_unwind(panic)));
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop_pondering();
    }
}

impl Search {
    /// Approfondit de `first` à `MAX_DEPTH`, jusqu'à l'échéance ou à l'arrêt demandé.
    fn choose(&mut self, board: &Board, color: Color, first: u8, rng: &mut impl Rng) -> Option<Choice> {
        let mut board = board.clone();
        let mut ordered = Vec::new();
        self.generate(&board, color, None, 0, &mut ordered);
        ordered.shuffle(rng);
        let &(_, only) = ordered.first()?;
        if ordered.len() == 1 {
            return Some(Choice { mv: only, depth: None });
        }
        ordered.sort_unstable_by_key(|&(score, _)| Reverse(score));
        let mut moves: Vec<Move> = ordered.into_iter().map(|(_, mv)| mv).collect();
        let mut best = Choice {
            mv: moves[0],
            depth: Some(0),
        };
        let step = if self.eval == Eval::Territory { usize::from(knob_u8(0)) } else { 1 };
        for depth in (first..=MAX_DEPTH).step_by(step) {
            self.age_history();
            let found = self.root(&mut board, &mut moves, depth);
            if let Some((mv, _)) = found {
                best.mv = mv;
            }
            if self.timed_out {
                break;
            }
            best.depth = Some(depth);
            if found.is_some_and(|(_, value)| value.abs() > WIN_BOUND) {
                break;
            }
        }
        Some(best)
    }
}

#[derive(Clone, Copy, Default)]
enum Bound {
    #[default]
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
struct Entry {
    mv: Option<Move>,
    value: i32,
    depth: u8,
    bound: Bound,
}

impl Entry {
    fn pack(self) -> u64 {
        let index = |square: Square| square.index() as u64;
        let mv = self.mv.map_or(0, |mv| {
            1 << 24 | index(mv.from) << 16 | index(mv.to) << 8 | index(mv.destroyed)
        });
        let value = u64::from((self.value + VALUE_OFFSET).cast_unsigned());
        mv | value << 25 | u64::from(self.depth) << 47 | (self.bound as u64) << 55
    }

    #[expect(clippy::cast_possible_truncation, reason = "champs extraits par masque")]
    fn unpack(data: u64) -> Self {
        let square = |shift: u32| Square::from_index(usize::from((data >> shift) as u8));
        let mv = || {
            Some(Move {
                from: square(16)?,
                to: square(8)?,
                destroyed: square(0)?,
            })
        };
        Self {
            mv: if data >> 24 & 1 == 1 { mv() } else { None },
            value: ((data >> 25) as u32 & ((1 << 22) - 1)).cast_signed() - VALUE_OFFSET,
            depth: (data >> 47) as u8,
            bound: match data >> 55 & 3 {
                0 => Bound::Exact,
                1 => Bound::Lower,
                _ => Bound::Upper,
            },
        }
    }
}

/// Table de transposition partagée entre fils, sans verrou : la clé est rangée masquée par les
/// données, si bien qu'une entrée déchirée par deux écritures simultanées ne se valide pas.
struct Table {
    cells: Vec<[AtomicU64; 2]>,
    deep: bool,
}

impl Table {
    fn new(deep: bool) -> Self {
        Self {
            cells: (0..TABLE_SIZE)
                .map(|_| [AtomicU64::new(0), AtomicU64::new(0)])
                .collect(),
            deep,
        }
    }

    fn get(&self, key: u64) -> Option<Entry> {
        let first = slot(key) & !usize::from(self.deep);
        self.cells[first..=first + usize::from(self.deep)]
            .iter()
            .find_map(|[check, data]| {
                let data = data.load(Ordering::Relaxed);
                (check.load(Ordering::Relaxed) ^ data == key).then(|| Entry::unpack(data))
            })
    }

    fn set(&self, key: u64, entry: Entry) {
        let mut index = slot(key) & !usize::from(self.deep);
        if self.deep {
            let [check, data] = &self.cells[index];
            let data = data.load(Ordering::Relaxed);
            if check.load(Ordering::Relaxed) ^ data != key && Entry::unpack(data).depth > entry.depth {
                index += 1;
            }
        }
        let data = entry.pack();
        let [check, word] = &self.cells[index];
        check.store(key ^ data, Ordering::Relaxed);
        word.store(data, Ordering::Relaxed);
    }
}

#[expect(clippy::cast_possible_truncation, reason = "les 20 bits bas du hash")]
const fn slot(key: u64) -> usize {
    key as usize & (TABLE_SIZE - 1)
}

struct Search {
    own: Color,
    eval: Eval,
    deadline: Instant,
    nodes: u64,
    timed_out: bool,
    hash: u64,
    table: Arc<Table>,
    stop: Arc<AtomicBool>,
    buffers: Vec<Vec<(i32, Move)>>,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: Vec<i32>,
    grid: Grid,
}

impl Search {
    #[cfg(test)]
    fn new(board: &Board, own: Color, eval: Eval, deadline: Instant) -> Self {
        Self::shared(
            board,
            own,
            eval,
            deadline,
            Arc::new(Table::new(eval == Eval::Territory)),
            Arc::default(),
        )
    }

    fn shared(
        board: &Board,
        own: Color,
        eval: Eval,
        deadline: Instant,
        table: Arc<Table>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            own,
            eval,
            deadline,
            nodes: 0,
            timed_out: false,
            hash: cpu::hash(board),
            table,
            stop,
            buffers: vec![Vec::new(); MAX_PLY],
            killers: [[None; 2]; MAX_PLY],
            history: vec![0; 2 * CELLS * CELLS],
            grid: Grid::new(board),
        }
    }

    fn reset(&mut self, board: &Board, own: Color, deadline: Instant) {
        self.own = own;
        self.deadline = deadline;
        self.nodes = 0;
        self.timed_out = false;
        self.hash = cpu::hash(board);
        self.killers = [[None; 2]; MAX_PLY];
        self.grid = Grid::new(board);
    }

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
        if depth == 1 {
            return match self.eval {
                Eval::LevelThree => self.frontier(board, side, ply, alpha, beta),
                Eval::Territory => self.frontier_territory(board, side, ply, alpha, beta),
            };
        }
        let mut table_move = None;
        if let Some(entry) = self.table.get(self.hash) {
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
        let mut moves = std::mem::take(&mut self.buffers[usize::from(ply)]);
        let staged = table_move.filter(|&mv| keeps_board_consistent(board, side, mv));
        if let Some(mv) = staged {
            moves.clear();
            moves.push((i32::MAX, mv));
        } else {
            self.generate(board, side, table_move, ply, &mut moves);
        }
        let Some(&(_, mut best_move)) = moves.first() else {
            self.buffers[usize::from(ply)] = moves;
            return loss(ply);
        };
        let original_alpha = alpha;
        let mut best = -INF;
        let mut index = 0;
        while index < moves.len() {
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
                let reduced = self.eval == Eval::Territory && depth >= LMR_DEPTH && index >= LMR_LATE;
                let mut value = if reduced {
                    -self.negamax(board, side.other(), depth - 1 - knob_u8(1), ply + 1, -alpha - 1, -alpha)
                } else {
                    alpha + 1
                };
                if value > alpha && !self.timed_out {
                    value = -self.negamax(board, side.other(), depth - 1, ply + 1, -alpha - 1, -alpha);
                }
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
            index += 1;
            if index == 1 && staged.is_some() {
                self.generate(board, side, table_move, ply, &mut moves);
                pick_best(&mut moves);
                if Some(moves[0].1) != staged {
                    index = 0;
                }
            }
        }
        self.buffers[usize::from(ply)] = moves;
        if self.timed_out {
            return 0;
        }
        self.store(best_move, best, depth, ply, bound(best, original_alpha, beta));
        best
    }

    fn frontier(&mut self, board: &mut Board, side: Color, ply: u8, alpha: i32, beta: i32) -> i32 {
        let (from, them) = (board.pawn(side), board.pawn(side.other()));
        let start = padded(from);
        self.grid.open[start] = true;
        let mut destinations = self.grid.destinations(from);
        let destinations = destinations.as_mut_slice();
        for killer in self.killers[usize::from(ply)].iter().rev().flatten() {
            if let Some(position) = destinations.iter().position(|&to| to == killer.to) {
                destinations[..=position].rotate_right(1);
            }
        }
        let destinations = &*destinations;
        let Some(targets) = targets(&self.grid, board.destroyed(), them, destinations) else {
            self.grid.open[start] = false;
            return loss(ply);
        };
        let targets = targets.as_slice();
        let destroyed = board.destroyed() + 1;
        let ring = rules::first_ring(destroyed);
        let opponent = Pawn::new(&self.grid, them);
        let gains = Gains::new(&opponent, targets, destroyed);
        let (mut best, mut best_move) = (-INF, None);
        let mut ceiling = -INF;
        'search: for &to in destinations {
            let mut moves = targets.iter().filter(|target| target.square != to).peekable();
            if moves.peek().is_none() {
                continue;
            }
            let arrival = Arrival::new(&self.grid, to, opponent, ring.clone());
            let bound = arrival.bound(&gains, destroyed);
            if bound <= best.max(alpha) {
                ceiling = ceiling.max(bound);
                continue;
            }
            for &target in moves {
                let mv = Move {
                    from,
                    to,
                    destroyed: target.square,
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
        self.grid.open[start] = false;
        let best = best.max(ceiling);
        if let Some(mv) = best_move
            && best >= beta
        {
            self.reward(side, mv, 1, ply);
        }
        best
    }

    /// `frontier` pour `Eval::Territory`. Les pions ne bloquant pas `Reach`, la valeur adverse ne
    /// dépend que de la cible et se calcule une fois par cible, à la demande ; la nôtre sans
    /// destruction borne celle de chaque cible, et on ne recalcule que les coups qui peuvent battre
    /// le meilleur. Les coups tueurs passent d'abord, pour couper avant d'avoir tout calculé.
    fn frontier_territory(&mut self, board: &Board, side: Color, ply: u8, alpha: i32, beta: i32) -> i32 {
        let (from, them) = (board.pawn(side), board.pawn(side.other()));
        let start = padded(from);
        self.grid.open[start] = true;
        let mut destinations = self.grid.destinations(from);
        let targets = targets(&self.grid, board.destroyed(), them, destinations.as_slice());
        self.grid.open[start] = false;
        let Some(targets) = targets else {
            return loss(ply);
        };
        let killers = self.killers[usize::from(ply)];
        let destinations = destinations.as_mut_slice();
        for killer in killers.iter().rev().flatten() {
            if let Some(position) = destinations.iter().position(|&to| to == killer.to) {
                destinations[..=position].rotate_right(1);
            }
        }
        let targets = targets.as_slice();
        let lifted = self.grid.empty | Bits::of(from);
        let free = lifted | Bits::of(them);
        let neighbours = SHIFTS
            .into_iter()
            .fold(Bits::default(), |all, shift| all | Bits::of(them).shift(shift));
        let mut theirs: [Option<Reach>; 16] = [None; 16];
        let (mut best, mut best_move, mut ceiling) = (-INF, None, -INF);
        'search: for &to in &*destinations {
            let mine = Reach::new(free, to);
            let bound = mine.value(them);
            let rings = rings(to, board.destroyed() + 1);
            let landed = lifted & !Bits::of(to);
            let mut order: Stack<usize, 16> = Stack::new(0);
            order.extend(0..targets.len());
            for killer in killers.iter().rev().flatten().filter(|killer| killer.to == to) {
                if let Some(position) = order
                    .as_slice()
                    .iter()
                    .position(|&i| targets[i].square == killer.destroyed)
                {
                    order.as_mut_slice()[..=position].rotate_right(1);
                }
            }
            for &index in order.as_slice() {
                let target = targets[index];
                if target.square == to {
                    continue;
                }
                let destroyed = Bits::of(target.square);
                let empty = landed & !destroyed;
                let their = *theirs[index].get_or_insert_with(|| Reach::new(free & !destroyed, them));
                let their_value = their.value(to);
                let value = if !opponent_moves(them, neighbours, empty, rings.as_slice()) {
                    -loss(ply + 1)
                } else if bound - their_value <= best.max(alpha) {
                    ceiling = ceiling.max(bound - their_value);
                    continue;
                } else if (mine.region() & destroyed).is_empty() {
                    bound - their_value
                } else {
                    Reach::new(free & !destroyed, to).value(them) - their_value
                };
                if value > best {
                    best = value;
                    best_move = Some(Move {
                        from,
                        to,
                        destroyed: target.square,
                    });
                    if value >= beta {
                        break 'search;
                    }
                }
            }
        }
        if let Some(mv) = best_move
            && best >= beta
        {
            self.reward(side, mv, 1, ply);
        }
        best.max(ceiling)
    }

    fn store(&self, mv: Move, value: i32, depth: u8, ply: u8, bound: Bound) {
        let entry = Entry {
            mv: Some(mv),
            value: to_table(value, ply),
            depth,
            bound,
        };
        self.table.set(self.hash, entry);
    }

    fn leaf(&self, board: &Board, side: Color, ply: u8) -> i32 {
        if self.eval == Eval::Territory {
            let (mine, theirs) = (board.pawn(side), board.pawn(side.other()));
            let empty = self.grid.empty;
            let near = reach(Bits::of(mine), empty);
            if !can_move(
                near,
                empty | Bits::of(mine),
                rings(theirs, board.destroyed()).as_slice(),
            ) {
                return loss(ply);
            }
            return territory(empty | Bits::of(mine) | Bits::of(theirs), mine, theirs);
        }
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

    fn generate(&mut self, board: &Board, side: Color, table_move: Option<Move>, ply: u8, out: &mut Vec<(i32, Move)>) {
        out.clear();
        let (from, them) = (board.pawn(side), board.pawn(side.other()));
        let opponent = Pawn::new(&self.grid, them);
        let destinations = self.grid.destinations(from);
        self.grid.open[padded(from)] = true;
        let targets = targets(&self.grid, board.destroyed(), them, destinations.as_slice());
        self.grid.open[padded(from)] = false;
        let Some(targets) = targets else {
            return;
        };
        let killers = self.killers[usize::from(ply)];
        for &to in destinations.as_slice() {
            let center = padded(to);
            let reach: i32 = STEPS
                .into_iter()
                .map(|step| i32::from(self.grid.ray(center, step)))
                .sum();
            for target in targets.as_slice().iter().filter(|target| target.square != to) {
                let mv = Move {
                    from,
                    to,
                    destroyed: target.square,
                };
                let score = if Some(mv) == table_move {
                    i32::MAX
                } else if Some(mv) == killers[0] {
                    i32::MAX - 1
                } else if Some(mv) == killers[1] {
                    i32::MAX - 2
                } else {
                    let threatened = target.distance.unsigned_abs() <= opponent.rays[target.direction];
                    self.history[history_index(side, mv)] + STATIC_WEIGHT * (reach + 8 * i32::from(threatened))
                };
                out.push((score, mv));
            }
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
        let every = if self.eval == Eval::Territory {
            CHECK_EVERY / 16
        } else {
            CHECK_EVERY
        };
        if self.nodes.is_multiple_of(every) && (Instant::now() > self.deadline || self.stop.load(Ordering::Relaxed)) {
            self.timed_out = true;
        }
        self.timed_out
    }

    fn play(&mut self, board: &mut Board, mv: Move) {
        self.hash ^= cpu::move_key(board, mv);
        board.apply(mv);
        self.grid.set(mv.from, true);
        self.grid.set(mv.to, false);
        self.grid.set(mv.destroyed, false);
    }

    fn unplay(&mut self, board: &mut Board, mv: Move) {
        board.undo(mv);
        self.hash ^= cpu::move_key(board, mv);
        self.grid.set(mv.destroyed, true);
        self.grid.set(mv.to, true);
        self.grid.set(mv.from, false);
    }
}

struct Grid {
    open: [bool; PADDED * PADDED],
    empty: Bits,
}

impl Grid {
    fn new(board: &Board) -> Self {
        let mut grid = Self {
            open: [false; PADDED * PADDED],
            empty: Bits::default(),
        };
        for (square, &cell) in (0..CELLS).filter_map(Square::from_index).zip(board.cells()) {
            grid.set(square, cell == Cell::Empty);
        }
        grid
    }

    fn set(&mut self, square: Square, open: bool) {
        self.open[padded(square)] = open;
        self.empty.set(square, open);
    }

    #[cfg(test)]
    fn matches(&self, board: &Board) -> bool {
        self.open == Self::new(board).open
    }

    fn destinations(&self, square: Square) -> Stack<Square, 24> {
        let start = padded(square);
        let mut destinations = Stack::new(square);
        for step in STEPS {
            let mut current = start;
            for _ in 0..self.ray(start, step) {
                current = current.wrapping_add_signed(step);
                destinations.extend(SQUARES[current]);
            }
        }
        destinations
    }

    fn ray(&self, start: usize, step: isize) -> u8 {
        let one = self.open[start.wrapping_add_signed(step)];
        let two = one & self.open[start.wrapping_add_signed(2 * step)];
        let three = two & self.open[start.wrapping_add_signed(3 * step)];
        u8::from(one) + u8::from(two) + u8::from(three)
    }
}

/// Ce qu'un pion atteint en un coup (`near`) et en deux (`far`) dans `free`, les cases non
/// détruites : les pions n'y bloquent rien, pour que la valeur d'un côté ne dépende pas de l'autre.
#[derive(Clone, Copy)]
struct Reach {
    near: Bits,
    far: Bits,
}

impl Reach {
    fn new(free: Bits, pawn: Square) -> Self {
        let near = reach(Bits::of(pawn), free);
        Self {
            near,
            far: reach(near, free) & !near & !Bits::of(pawn),
        }
    }

    fn region(self) -> Bits {
        self.near | self.far
    }

    /// Sans compter `other`, la case du pion adverse.
    fn value(self, other: Square) -> i32 {
        let other = !Bits::of(other);
        KNOBS[2] * (self.near & other).count() + KNOBS[3] * (self.far & other).count()
    }
}

fn territory(free: Bits, mine: Square, theirs: Square) -> i32 {
    Reach::new(free, mine).value(theirs) - Reach::new(free, theirs).value(mine)
}

/// Anneaux de destruction autour de `center`, dans l'ordre de repli de `rules::destroy_rings`.
fn rings(center: Square, destroyed: u32) -> Stack<Bits, 3> {
    let center = Bits::of(center);
    let mut rings = Stack::new(Bits::default());
    for ring in rules::destroy_rings(destroyed) {
        let ring = ring
            .clone()
            .flat_map(|k| SHIFTS.map(|shift| center.shift(shift * i32::from(k))));
        rings.push(ring.fold(Bits::default(), |all, square| all | square));
    }
    rings
}

/// Comme `rules::has_moves` : `open` compte la case de départ du joueur comme vide.
fn can_move(destinations: Bits, open: Bits, rings: &[Bits]) -> bool {
    !destinations.is_empty()
        && rings.iter().any(|&ring| {
            let targets = ring & open;
            match targets.count() {
                0 => false,
                1 => targets != destinations,
                _ => true,
            }
        })
}

/// `can_move` du pion adverse, dont les destinations ne se calculent que s'il a moins de deux
/// voisines libres : sinon il en a au moins deux.
fn opponent_moves(them: Square, neighbours: Bits, empty: Bits, rings: &[Bits]) -> bool {
    let open = empty | Bits::of(them);
    match (neighbours & empty).count() {
        0 => false,
        1 => can_move(reach(Bits::of(them), empty), open, rings),
        _ => rings.iter().any(|&ring| !(ring & open).is_empty()),
    }
}

/// Cases où un coup mène depuis l'une de `from` : jusqu'à 3 pas en ligne droite dans `empty`.
fn reach(from: Bits, empty: Bits) -> Bits {
    ray::<-17>(from, empty)
        | ray::<-16>(from, empty)
        | ray::<-15>(from, empty)
        | ray::<-1>(from, empty)
        | ray::<1>(from, empty)
        | ray::<15>(from, empty)
        | ray::<16>(from, empty)
        | ray::<17>(from, empty)
}

#[expect(clippy::inline_always, reason = "chaque décalage se résout à la compilation")]
#[inline(always)]
fn ray<const BY: i32>(from: Bits, empty: Bits) -> Bits {
    // ±1 reste dans la rangée : sa retenue entre mots tomberait sur une colonne de garde.
    let step = |bits: Bits| match BY {
        1 => Bits(bits.0.map(|word| word << 1)),
        -1 => Bits(bits.0.map(|word| word >> 1)),
        _ => bits.shift(BY),
    };
    let one = step(from) & empty;
    let two = step(one) & empty;
    one | two | (step(two) & empty)
}

/// Décalages des 8 directions dans `Bits`, rangées de 16 bits.
const SHIFTS: [i32; 8] = [-17, -16, -15, -1, 1, 15, 16, 17];

/// Une case par bit, 16 bits par rangée : les colonnes 12 à 15 restent vides et arrêtent les
/// décalages qui débordent sur le côté.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Bits([u64; 3]);

impl Bits {
    fn of(square: Square) -> Self {
        let mut bits = Self::default();
        bits.set(square, true);
        bits
    }

    fn set(&mut self, square: Square, on: bool) {
        let index = usize::from(square.y()) * 16 + usize::from(square.x());
        let (word, bit) = (index / 64, 1 << (index % 64));
        if on {
            self.0[word] |= bit;
        } else {
            self.0[word] &= !bit;
        }
    }

    #[expect(clippy::inline_always, reason = "appelé avec un décalage constant")]
    #[inline(always)]
    fn shift(self, by: i32) -> Self {
        let [a, b, c] = self.0;
        let s = by.unsigned_abs();
        if by > 0 {
            Self([a << s, b << s | a >> (64 - s), c << s | b >> (64 - s)])
        } else {
            Self([a >> s | b << (64 - s), b >> s | c << (64 - s), c >> s])
        }
    }

    fn count(self) -> i32 {
        self.0.iter().map(|word| word.count_ones().cast_signed()).sum()
    }

    fn is_empty(self) -> bool {
        self.0 == [0; 3]
    }
}

impl std::ops::BitAnd for Bits {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        Self([self.0[0] & other.0[0], self.0[1] & other.0[1], self.0[2] & other.0[2]])
    }
}

impl std::ops::BitOr for Bits {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self([self.0[0] | other.0[0], self.0[1] | other.0[1], self.0[2] | other.0[2]])
    }
}

impl std::ops::Not for Bits {
    type Output = Self;
    fn not(self) -> Self {
        Self(self.0.map(|word| !word))
    }
}

fn padded(square: Square) -> usize {
    (usize::from(square.y()) + 3) * PADDED + usize::from(square.x()) + 3
}

#[expect(clippy::cast_possible_truncation, reason = "moins de 256 lignes et colonnes")]
const SQUARES: [Option<Square>; PADDED * PADDED] = {
    let mut squares = [None; PADDED * PADDED];
    let mut index = 0;
    while index < PADDED * PADDED {
        let (x, y) = ((index % PADDED) as u8, (index / PADDED) as u8);
        squares[index] = Square::new(x.wrapping_sub(3), y.wrapping_sub(3));
        index += 1;
    }
    squares
};

struct Stack<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy, const N: usize> Stack<T, N> {
    const fn new(filler: T) -> Self {
        Self {
            items: [filler; N],
            len: 0,
        }
    }

    fn push(&mut self, item: T) {
        self.items[self.len] = item;
        self.len += 1;
    }

    fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.push(item);
        }
    }

    fn as_slice(&self) -> &[T] {
        &self.items[..self.len]
    }

    fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
}

#[derive(Clone, Copy)]
struct Target {
    square: Square,
    direction: usize,
    distance: i8,
}

fn targets(grid: &Grid, destroyed: u32, them: Square, destinations: &[Square]) -> Option<Stack<Target, 16>> {
    let center = padded(them);
    rules::destroy_rings(destroyed).iter().find_map(|ring| {
        let mut targets = Stack::new(Target {
            square: them,
            direction: 0,
            distance: 0,
        });
        for (direction, step) in STEPS.into_iter().enumerate() {
            for distance in ring.clone() {
                let index = center.wrapping_add_signed(step * isize::from(distance));
                if let Some(square) = SQUARES[index].filter(|_| grid.open[index]) {
                    targets.push(Target {
                        square,
                        direction,
                        distance,
                    });
                }
            }
        }
        let playable = match targets.as_slice() {
            [] => false,
            [only] => destinations.iter().any(|&to| to != only.square),
            _ => !destinations.is_empty(),
        };
        playable.then_some(targets)
    })
}

#[derive(Clone, Copy)]
struct Pawn {
    square: Square,
    rays: [u8; 8],
    mobility: i32,
    free: i32,
}

impl Pawn {
    fn new(grid: &Grid, square: Square) -> Self {
        let center = padded(square);
        let rays = STEPS.map(|step| grid.ray(center, step));
        Self {
            square,
            rays,
            mobility: rays.iter().map(|&length| i32::from(length)).sum(),
            free: rays.iter().map(|&length| i32::from(length > 0)).sum(),
        }
    }

    fn blocked(mut self, square: Square) -> Self {
        if let Some((direction, distance)) = alignment(self.square, square) {
            let length = &mut self.rays[direction];
            if distance.unsigned_abs() <= *length {
                let shorter = distance.unsigned_abs() - 1;
                self.mobility -= i32::from(*length - shorter);
                *length = shorter;
            }
            self.free -= i32::from(distance == 1);
        }
        self
    }
}

struct Gains {
    best: (i32, Square),
    second: i32,
    cut: i32,
}

impl Gains {
    fn new(opponent: &Pawn, targets: &[Target], destroyed: u32) -> Self {
        let mut gains = Self {
            best: (i32::MIN, opponent.square),
            second: i32::MIN,
            cut: 0,
        };
        for target in targets {
            let cut = cut(Some((target.direction, target.distance)), opponent.rays);
            let gain = cpu::level_three(destroyed, 0, cut, i32::from(target.distance == 1));
            if gain > gains.best.0 {
                gains.second = gains.best.0;
                gains.best = (gain, target.square);
            } else {
                gains.second = gains.second.max(gain);
            }
            gains.cut = gains.cut.max(cut);
        }
        gains
    }
}

struct Arrival {
    to: Square,
    mine: Pawn,
    theirs: Pawn,
    ring: RangeInclusive<i8>,
    ring_count: i32,
    edges: i32,
}

impl Arrival {
    fn new(grid: &Grid, to: Square, opponent: Pawn, ring: RangeInclusive<i8>) -> Self {
        let center = padded(to);
        let them = padded(opponent.square);
        let mut ring_count = 0;
        for step in STEPS {
            for k in ring.clone() {
                let square = center.wrapping_add_signed(step * isize::from(k));
                ring_count += i32::from(square == them || grid.open[square]);
            }
        }
        Self {
            to,
            mine: Pawn::new(grid, to),
            theirs: opponent.blocked(to),
            ring,
            ring_count,
            edges: cpu::edge_distance(to) - cpu::edge_distance(opponent.square),
        }
    }

    fn value(&self, target: Target, destroyed: u32, ply: u8) -> Option<i32> {
        let theirs = self.theirs.mobility - cut(Some((target.direction, target.distance)), self.theirs.rays);
        if theirs == 0 {
            return Some(-loss(ply + 1));
        }
        let line = alignment(self.to, target.square);
        let in_ring = line.is_some_and(|(_, distance)| self.ring.contains(&distance));
        if self.ring_count - i32::from(in_ring) < 2 {
            return None;
        }
        let mine = self.mine.mobility - cut(line, self.mine.rays);
        let free =
            (self.mine.free - adjacent(target.square, self.to)) - (self.theirs.free - i32::from(target.distance == 1));
        Some(cpu::level_three(destroyed, self.edges, mine - theirs, free))
    }

    fn bound(&self, gains: &Gains, destroyed: u32) -> i32 {
        if self.ring_count <= 2 || self.theirs.mobility <= gains.cut {
            return INF;
        }
        let gain = if gains.best.1 == self.to {
            gains.second
        } else {
            gains.best.0
        };
        let (mobility, free) = (
            self.mine.mobility - self.theirs.mobility,
            self.mine.free - self.theirs.free,
        );
        cpu::level_three(destroyed, self.edges, mobility, free) + gain
    }
}

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

fn keeps_board_consistent(board: &Board, side: Color, mv: Move) -> bool {
    mv.from == board.pawn(side)
        && mv.to != mv.destroyed
        && board.is_empty(mv.to)
        && (mv.destroyed == mv.from || board.is_empty(mv.destroyed))
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
    use rand::seq::IndexedRandom;

    use super::*;
    use crate::board::DIRS;
    use crate::board::test_support::diagram;
    use crate::rules::test_support::positions;

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    fn brute(search: &mut Search, board: &mut Board, side: Color, depth: u8, ply: u8) -> i32 {
        if depth == 0 {
            return search.leaf(board, side, ply);
        }
        rules::legal_moves(board, side)
            .into_iter()
            .map(|mv| {
                search.play(board, mv);
                let value = -brute(search, board, side.other(), depth - 1, ply + 1);
                search.unplay(board, mv);
                value
            })
            .max()
            .unwrap_or_else(|| loss(ply))
    }

    fn assert_root_matches_brute_force(board: &Board, color: Color, eval: Eval, depth: u8) {
        let mut scratch = board.clone();
        let mut search = Search::new(&scratch, color, eval, far());
        let start = search.hash;
        let mut moves = rules::legal_moves(board, color);
        for depth in 1..=depth {
            let (_, value) = search.root(&mut scratch, &mut moves, depth).unwrap();
            let expected = brute(&mut search, &mut board.clone(), color, depth, 0);
            assert_eq!(value, expected, "profondeur {depth}\n{board}");
        }
        assert_eq!(&scratch, board);
        assert_eq!(search.hash, start);
        assert!(search.grid.matches(board));
    }

    #[test]
    fn root_value_matches_brute_force() {
        for eval in [Eval::LevelThree, Eval::Territory] {
            for (board, color) in positions(11, 6, 30) {
                assert_root_matches_brute_force(&board, color, eval, 2);
            }
            for (board, color) in positions(5, 4, 12) {
                assert_root_matches_brute_force(&board, color, eval, 3);
            }
        }
        for game in 0..12 {
            let opening = rules::random_opening(2 + 4 * game, &mut StdRng::seed_from_u64(game as u64));
            if let Ok(replay) = rules::Replay::from_moves(opening)
                && replay.winner.is_none()
            {
                assert_root_matches_brute_force(&replay.board, replay.turn, Eval::Territory, 2);
            }
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
            let search = Search::new(&board, color, Eval::LevelThree, far());
            let (from, them) = (board.pawn(color), board.pawn(color.other()));
            let destroyed = board.destroyed() + 1;
            let mut grid = Grid::new(&board);
            grid.open[padded(from)] = true;
            let opponent = Pawn::new(&grid, them);
            let destinations: Vec<Square> = rules::legal_moves(&board, color).iter().map(|mv| mv.to).collect();
            let targets = targets(&grid, board.destroyed(), them, &destinations).unwrap();
            let gains = Gains::new(&opponent, targets.as_slice(), destroyed);
            for mv in rules::legal_moves(&board, color) {
                let arrival = Arrival::new(&grid, mv.to, opponent, rules::first_ring(destroyed));
                let mut after = board.clone();
                after.apply(mv);
                let expected = -search.leaf(&after, color.other(), 1);
                assert!(expected <= arrival.bound(&gains, destroyed), "{mv}\n{board}");
                let (direction, distance) = alignment(them, mv.destroyed).unwrap();
                let target = Target {
                    square: mv.destroyed,
                    direction,
                    distance,
                };
                if let Some(value) = arrival.value(target, destroyed, 0) {
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
    fn generate_follows_the_rules() {
        let mut boards = positions(11, 6, 30);
        boards.extend(positions(4, 30, 60));
        for (board, color) in boards {
            let mut search = Search::new(&board, color, Eval::LevelThree, far());
            let mut scored = Vec::new();
            search.generate(&board, color, None, 0, &mut scored);
            let moves: Vec<Move> = scored.into_iter().map(|(_, mv)| mv).collect();
            assert_eq!(moves, rules::legal_moves(&board, color), "\n{board}");
            assert!(search.grid.matches(&board));
        }
    }

    #[test]
    fn reach_follows_destinations() {
        let mut boards = positions(11, 6, 30);
        boards.extend(positions(4, 30, 60));
        for (board, color) in boards {
            let grid = Grid::new(&board);
            let from = board.pawn(color);
            let mut expected = Bits::default();
            rules::destinations(&board, from, |to| expected.set(to, true));
            assert!(
                reach(Bits::of(from), grid.empty) == expected,
                "
{board}"
            );
            let mut second = Bits::default();
            rules::destinations(&board, from, |to| {
                rules::destinations(&board, to, |next| second.set(next, true));
            });
            assert!(
                reach(expected, grid.empty) == second,
                "
{board}"
            );
        }
    }

    #[test]
    fn has_moves_follows_the_rules() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..300 {
            let (mut board, mut turn) = (Board::initial(), Color::Red);
            loop {
                let (from, them) = (board.pawn(turn), board.pawn(turn.other()));
                let expected = rules::has_moves(&board, turn);
                let empty = Grid::new(&board).empty;
                let rings = rings(them, board.destroyed());
                let moves = can_move(reach(Bits::of(from), empty), empty | Bits::of(from), rings.as_slice());
                assert_eq!(
                    moves, expected,
                    "
{board}"
                );
                let Some(&mv) = rules::legal_moves(&board, turn).choose(&mut rng) else {
                    break;
                };
                board.apply(mv);
                turn = turn.other();
            }
        }
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
        let choice = choose(
            &board,
            Color::Red,
            Eval::LevelThree,
            far(),
            &mut StdRng::seed_from_u64(1),
        )
        .unwrap();
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
        let choice = choose(
            &Board::initial(),
            Color::Red,
            Eval::LevelThree,
            deadline,
            &mut StdRng::seed_from_u64(2),
        )
        .unwrap();
        assert!(rules::is_legal(&Board::initial(), Color::Red, choice.mv));
        assert!(start.elapsed() < Duration::from_millis(400), "{:?}", start.elapsed());
    }

    #[test]
    #[ignore = "mesure de vitesse, à lancer en release"]
    fn bench() {
        for eval in [Eval::LevelThree, Eval::Territory] {
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
                    let mut search = Search::new(&board, replay.turn, eval, far());
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
            println!("{eval:?} : {nodes} nœuds en {elapsed:.2?}, {rate:.0} k nœuds/s");
        }
    }
}
