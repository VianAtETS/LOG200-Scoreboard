#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Seed(usize),
    Winner(usize),
    Loser(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Winners,
    Losers,
    Final,
}

#[derive(Clone, Debug)]
pub struct Match {
    pub id: String,
    pub side: Side,
    pub round: u32,
    pub sources: [Source; 2],
    pub best_of: u32,
    pub head_start: [u32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Waiting,
    Ready([usize; 2]),
    /// `None` : une exemption.
    Done {
        winner: Option<usize>,
        loser: Option<usize>,
    },
}

pub struct Bracket {
    pub teams: usize,
    pub matches: Vec<Match>,
}

const BEST_OF: u32 = 3;
const FINAL_BEST_OF: u32 = 5;

impl Bracket {
    pub fn new(teams: usize) -> Self {
        assert!(teams >= 2, "un tableau demande au moins deux équipes");
        let mut order = vec![0];
        while order.len() < teams.next_power_of_two() {
            let n = order.len() * 2;
            order = order.iter().flat_map(|&s| [s, n - 1 - s]).collect();
        }
        let mut matches = Vec::new();
        let mut add = |side, round, sources| {
            let (best_of, head_start) = if side == Side::Final {
                (FINAL_BEST_OF, [1, 0])
            } else {
                (BEST_OF, [0, 0])
            };
            matches.push(Match {
                id: String::new(),
                side,
                round,
                sources,
                best_of,
                head_start,
            });
            matches.len() - 1
        };

        let mut winners: Vec<Vec<usize>> = vec![
            order
                .chunks(2)
                .map(|p| add(Side::Winners, 1, [Source::Seed(p[0]), Source::Seed(p[1])]))
                .collect(),
        ];
        while let [.., last] = winners.as_slice()
            && last.len() > 1
        {
            let round = u32::try_from(winners.len()).unwrap_or(u32::MAX) + 1;
            let next = last
                .clone()
                .chunks(2)
                .map(|p| add(Side::Winners, round, [Source::Winner(p[0]), Source::Winner(p[1])]))
                .collect();
            winners.push(next);
        }

        let mut round = 1;
        let mut survivors: Vec<Source> = match winners[0].as_slice() {
            [only] => vec![Source::Loser(*only)],
            first => first
                .chunks(2)
                .map(|p| Source::Winner(add(Side::Losers, round, [Source::Loser(p[0]), Source::Loser(p[1])])))
                .collect(),
        };
        for dropped in &winners[1..] {
            round += 1;
            // Ordre inverse : pas de revanche immédiate.
            let n = dropped.len();
            survivors = survivors
                .iter()
                .enumerate()
                .map(|(j, &s)| Source::Winner(add(Side::Losers, round, [s, Source::Loser(dropped[n - 1 - j])])))
                .collect();
            if survivors.len() > 1 {
                round += 1;
                survivors = survivors
                    .chunks(2)
                    .map(|p| Source::Winner(add(Side::Losers, round, [p[0], p[1]])))
                    .collect();
            }
        }
        let final_ = winners[winners.len() - 1][0];
        add(Side::Final, 1, [Source::Winner(final_), survivors[0]]);

        let mut numbers = [0; 2];
        for m in &mut matches {
            m.id = match m.side {
                Side::Winners => {
                    numbers[0] += 1;
                    format!("W{}", numbers[0])
                }
                Side::Losers => {
                    numbers[1] += 1;
                    format!("L{}", numbers[1])
                }
                Side::Final => "GF".into(),
            };
        }
        Self { teams, matches }
    }

    pub fn statuses(&self, wins: &[[u32; 2]]) -> Vec<Status> {
        let mut out: Vec<Status> = Vec::with_capacity(self.matches.len());
        for (k, m) in self.matches.iter().enumerate() {
            let entrant = |source| match source {
                Source::Seed(s) => Some((s < self.teams).then_some(s)),
                Source::Winner(j) | Source::Loser(j) => match out[j] {
                    Status::Done { winner, loser } => Some(if matches!(source, Source::Winner(_)) {
                        winner
                    } else {
                        loser
                    }),
                    _ => None,
                },
            };
            let status = match m.sources.map(entrant) {
                [Some(None), Some(None)] => Status::Done {
                    winner: None,
                    loser: None,
                },
                [Some(Some(team)), Some(None)] | [Some(None), Some(Some(team))] => Status::Done {
                    winner: Some(team),
                    loser: None,
                },
                [Some(Some(a)), Some(Some(b))] => {
                    let w = wins.get(k).copied().unwrap_or_default();
                    let needed = m.best_of / 2 + 1;
                    if w[0] + m.head_start[0] >= needed {
                        Status::Done {
                            winner: Some(a),
                            loser: Some(b),
                        }
                    } else if w[1] + m.head_start[1] >= needed {
                        Status::Done {
                            winner: Some(b),
                            loser: Some(a),
                        }
                    } else {
                        Status::Ready([a, b])
                    }
                }
                _ => Status::Waiting,
            };
            out.push(status);
        }
        out
    }

    pub fn placements(&self, statuses: &[Status]) -> Option<Vec<usize>> {
        let Status::Done {
            winner: Some(champion),
            loser: Some(runner_up),
        } = *statuses.last()?
        else {
            return None;
        };
        let mut places = vec![0; self.teams];
        places[champion] = 1;
        places[runner_up] = 2;
        let mut next = 3;
        let last_round = self.matches.iter().map(|m| m.round).max().unwrap_or(0);
        for round in (1..=last_round).rev() {
            let out: Vec<usize> = self
                .matches
                .iter()
                .zip(statuses)
                .filter(|(m, _)| m.side == Side::Losers && m.round == round)
                .filter_map(|(_, s)| match s {
                    Status::Done { loser: Some(l), .. } => Some(*l),
                    _ => None,
                })
                .collect();
            for &l in &out {
                places[l] = next;
            }
            next += out.len();
        }
        Some(places)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};

    use super::*;

    fn play(bracket: &Bracket, mut game: impl FnMut(&Match, [usize; 2]) -> usize) -> (Vec<Status>, Vec<[u32; 2]>) {
        let mut wins = vec![[0; 2]; bracket.matches.len()];
        loop {
            let statuses = bracket.statuses(&wins);
            let ready: Vec<(usize, [usize; 2])> = statuses
                .iter()
                .enumerate()
                .filter_map(|(k, s)| match s {
                    Status::Ready(seeds) => Some((k, *seeds)),
                    _ => None,
                })
                .collect();
            if ready.is_empty() {
                return (statuses, wins);
            }
            for (k, seeds) in ready {
                wins[k][game(&bracket.matches[k], seeds)] += 1;
            }
        }
    }

    fn favourite(_: &Match, seeds: [usize; 2]) -> usize {
        usize::from(seeds[1] < seeds[0])
    }

    fn ready(statuses: &[Status]) -> Vec<[usize; 2]> {
        statuses
            .iter()
            .filter_map(|s| match s {
                Status::Ready(seeds) => Some(*seeds),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn first_round_pairs_strong_against_weak() {
        let bracket = Bracket::new(8);
        assert_eq!(ready(&bracket.statuses(&[])), [[0, 7], [3, 4], [1, 6], [2, 5]]);
    }

    #[test]
    fn byes_go_to_the_top_seeds_without_a_game() {
        let bracket = Bracket::new(5);
        let statuses = bracket.statuses(&[]);
        assert_eq!(ready(&statuses), [[3, 4], [1, 2]]);
        assert_eq!(
            statuses[0],
            Status::Done {
                winner: Some(0),
                loser: None
            }
        );
    }

    #[test]
    fn a_best_of_three_needs_two_wins() {
        let bracket = Bracket::new(4);
        assert_eq!(bracket.statuses(&[[1, 1]])[0], Status::Ready([0, 3]));
        assert_eq!(
            bracket.statuses(&[[1, 2]])[0],
            Status::Done {
                winner: Some(3),
                loser: Some(0)
            }
        );
    }

    #[test]
    fn grand_final_is_a_best_of_five_with_a_one_game_lead() {
        let bracket = Bracket::new(4);
        let gf = bracket.matches.len() - 1;
        let m = &bracket.matches[gf];
        assert_eq!((m.side, m.best_of, m.head_start), (Side::Final, 5, [1, 0]));
        let (_, mut wins) = play(&bracket, favourite);
        wins[gf] = [2, 0];
        let statuses = bracket.statuses(&wins);
        assert_eq!(
            statuses[gf],
            Status::Done {
                winner: Some(0),
                loser: Some(1)
            }
        );
        wins[gf] = [1, 2];
        assert_eq!(bracket.statuses(&wins)[gf], Status::Ready([0, 1]));
        wins[gf] = [1, 3];
        assert_eq!(
            bracket.statuses(&wins)[gf],
            Status::Done {
                winner: Some(1),
                loser: Some(0)
            }
        );
    }

    #[test]
    fn two_teams_meet_again_in_the_grand_final() {
        let bracket = Bracket::new(2);
        let (statuses, _) = play(&bracket, favourite);
        assert_eq!(bracket.placements(&statuses), Some(vec![1, 2]));
        assert_eq!(bracket.matches.last().map(|m| m.side), Some(Side::Final));
    }

    #[test]
    fn favourites_finish_in_seed_order_at_the_top() {
        for teams in [2, 3, 4, 5, 8, 13, 16] {
            let bracket = Bracket::new(teams);
            let (statuses, _) = play(&bracket, favourite);
            let places = bracket.placements(&statuses).unwrap();
            assert_eq!(places.len(), teams);
            for (seed, &place) in places.iter().enumerate().take(teams.min(3)) {
                assert_eq!(place, seed + 1, "{teams} équipes : {places:?}");
            }
        }
    }

    #[test]
    fn every_bracket_finishes_with_consistent_places_and_two_losses_out() {
        for teams in 2..=20 {
            for seed in 0..20 {
                let bracket = Bracket::new(teams);
                let mut rng = StdRng::seed_from_u64(seed);
                let (statuses, _) = play(&bracket, |_, _| usize::from(rng.random::<bool>()));
                assert!(
                    statuses.iter().all(|s| matches!(s, Status::Done { .. })),
                    "{teams} équipes"
                );
                let places = bracket.placements(&statuses).unwrap();
                for &p in &places {
                    assert_eq!(p, 1 + places.iter().filter(|&&q| q < p).count(), "{places:?}");
                }
                let mut losses: HashMap<usize, u32> = HashMap::new();
                for s in &statuses {
                    if let Status::Done {
                        loser: Some(l),
                        winner: Some(_),
                    } = s
                    {
                        *losses.entry(*l).or_default() += 1;
                    }
                }
                let runner_up = places.iter().position(|&p| p == 2).unwrap();
                for (team, &place) in places.iter().enumerate() {
                    let n = losses.get(&team).copied().unwrap_or(0);
                    match place {
                        1 => assert!(n <= 1, "{teams} équipes, champion : {n} défaites"),
                        _ if team == runner_up => assert!((1..=2).contains(&n)),
                        _ => assert_eq!(n, 2, "{teams} équipes, tête {team}, place {place}"),
                    }
                }
            }
        }
    }

    #[test]
    fn first_drop_in_round_avoids_rematches() {
        for teams in [8, 16] {
            for seed in 0..20 {
                let bracket = Bracket::new(teams);
                let mut rng = StdRng::seed_from_u64(seed);
                let (statuses, _) = play(&bracket, |_, _| usize::from(rng.random::<bool>()));
                let met: HashSet<[usize; 2]> = bracket
                    .matches
                    .iter()
                    .zip(&statuses)
                    .filter(|(m, _)| m.side == Side::Winners)
                    .filter_map(|(_, s)| match s {
                        Status::Done {
                            winner: Some(w),
                            loser: Some(l),
                        } => Some([*w.min(l), *w.max(l)]),
                        _ => None,
                    })
                    .collect();
                for (m, s) in bracket.matches.iter().zip(&statuses) {
                    if m.side == Side::Losers
                        && m.round == 2
                        && let Status::Done {
                            winner: Some(w),
                            loser: Some(l),
                        } = s
                    {
                        assert!(!met.contains(&[*w.min(l), *w.max(l)]), "{} : revanche", m.id);
                    }
                }
            }
        }
    }

    #[test]
    fn ids_number_each_side_in_round_order() {
        let ids: Vec<String> = Bracket::new(8).matches.into_iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            [
                "W1", "W2", "W3", "W4", "W5", "W6", "W7", "L1", "L2", "L3", "L4", "L5", "L6", "GF"
            ]
        );
    }
}
