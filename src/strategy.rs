//! Stratégies des bots : bots de test, et répliques du CPU du serveur officiel (`cpu`).

use std::time::Instant;

use rand::Rng;
use rand::seq::IndexedRandom;

use crate::board::{Board, Color, Move};
use crate::cpu::{self, Choice, Level};
use crate::rules::{self, mobility};

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Strategy {
    /// Un coup légal au hasard.
    Random,
    /// Gagne tout de suite si possible, sinon maximise sa mobilité moins celle de l'adversaire ;
    /// hasard entre les ex aequo.
    Greedy,
    /// CPU "Aléatoire" du serveur officiel : un coup légal au hasard, comme `random`.
    #[value(name = "aleatoire")]
    Aleatoire,
    /// CPU "Niveau 1" du serveur officiel : minimax alpha-bêta, deux demi-coups après le sien.
    #[value(name = "niveau-1")]
    Niveau1,
    /// CPU "Niveau 2" du serveur officiel : le même minimax, approfondi tant que le temps le permet.
    #[value(name = "niveau-2")]
    Niveau2,
    /// CPU "Niveau 3" du serveur officiel : negamax avec table de transposition, approfondi tant que
    /// le temps le permet ; tient aussi compte du bord et des cases voisines libres.
    #[value(name = "niveau-3")]
    Niveau3,
}

impl Strategy {
    pub fn choose(self, board: &Board, color: Color, deadline: Instant, rng: &mut impl Rng) -> Option<Choice> {
        let unsearched = |mv| Choice { mv, depth: None };
        match self {
            Self::Random | Self::Aleatoire => rules::legal_moves(board, color).choose(rng).copied().map(unsearched),
            Self::Greedy => greedy(board, color, &rules::legal_moves(board, color), rng).map(unsearched),
            Self::Niveau1 => cpu::choose(board, color, Level::One, deadline, rng),
            Self::Niveau2 => cpu::choose(board, color, Level::Two, deadline, rng),
            Self::Niveau3 => cpu::choose(board, color, Level::Three, deadline, rng),
        }
    }
}

fn greedy(board: &Board, color: Color, moves: &[Move], rng: &mut impl Rng) -> Option<Move> {
    let mut board = board.clone();
    let mut best = Vec::new();
    let mut best_score = i32::MIN;
    for &mv in moves {
        board.apply(mv);
        let won = !rules::has_moves(&board, color.other());
        let score = mobility(&board, color) - mobility(&board, color.other());
        board.undo(mv);
        if won {
            return Some(mv);
        }
        if score > best_score {
            best_score = score;
            best.clear();
        }
        if score == best_score {
            best.push(mv);
        }
    }
    best.choose(rng).copied()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::board::test_support::diagram;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn all_strategies_play_legal_moves() {
        use clap::ValueEnum;
        let b = Board::initial();
        let mut rng = StdRng::seed_from_u64(1);
        for &s in Strategy::value_variants() {
            let deadline = Instant::now() + Duration::from_millis(200);
            let m = s.choose(&b, Color::Red, deadline, &mut rng).unwrap().mv;
            assert!(rules::is_legal(&b, Color::Red, m), "{s:?}");
        }
    }

    #[test]
    fn greedy_takes_immediate_win() {
        // Noir en A12 n'a qu'une sortie (B11) : rouge gagne en la détruisant.
        let b = diagram(
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
        let mut rng = StdRng::seed_from_u64(1);
        let m = Strategy::Greedy
            .choose(&b, Color::Red, Instant::now(), &mut rng)
            .unwrap()
            .mv;
        let mut after = b.clone();
        after.apply(m);
        assert_eq!(rules::winner_after(&after, Color::Red), Some(Color::Red), "{m}");
    }

    #[test]
    fn greedy_beats_random() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut greedy_wins = 0;
        for game in 0..20 {
            let greedy_color = if game % 2 == 0 { Color::Red } else { Color::Black };
            let mut b = Board::initial();
            let mut turn = Color::Red;
            let winner = loop {
                let s = if turn == greedy_color {
                    Strategy::Greedy
                } else {
                    Strategy::Random
                };
                let m = s
                    .choose(&b, turn, Instant::now(), &mut rng)
                    .expect("le joueur au trait a toujours un coup")
                    .mv;
                b.apply(m);
                if let Some(w) = rules::winner_after(&b, turn) {
                    break w;
                }
                turn = turn.other();
            };
            greedy_wins += u32::from(winner == greedy_color);
        }
        assert!(greedy_wins >= 16, "glouton : {greedy_wins}/20");
    }

    #[test]
    fn parses_names() {
        use clap::ValueEnum;
        assert_eq!(Strategy::from_str("greedy", false), Ok(Strategy::Greedy));
        assert_eq!(Strategy::from_str("aleatoire", false), Ok(Strategy::Aleatoire));
        assert_eq!(Strategy::from_str("niveau-2", false), Ok(Strategy::Niveau2));
        assert!(Strategy::from_str("2", false).is_err());
    }
}
