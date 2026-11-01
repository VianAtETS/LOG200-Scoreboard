//! Probabilité de victoire Bradley–Terry : une force par équipe et l'avantage du rouge, ajustées sur
//! l'historique et publiées dans `model.json` (format dans docs/formats.md).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use isolation_core::board::Color;
use isolation_core::record::{GameRecord, Reason};

use crate::store::write_atomic;

/// Parties entre deux ajustements.
const REFIT_EVERY: u32 = 25;
/// Pénalité L2 sur les forces : une équipe qui n'a jamais perdu garde une force finie.
const RIDGE: f64 = 1.0;
/// Parties après lesquelles le poids d'une partie est divisé par deux.
const HALF_LIFE: f64 = 500.0;
/// Une partie sur `HOLDOUT_DIVISOR` (les plus récentes) sert à valider le modèle.
const HOLDOUT_DIVISOR: usize = 5;
/// Plafond : chaque tranche de validation coûte un ajustement complet.
const HOLDOUT_MAX: usize = 1000;
const HOLDOUT_MIN: usize = 30;
/// Parties qu'une équipe doit avoir jouées pour qu'on affiche une probabilité.
const MIN_GAMES: u32 = 10;
const HISTORY: usize = 20;
const NEWTON_STEPS: usize = 30;
const STEP_MAX: f64 = 5.0;

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn round4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

struct Sample {
    red: String,
    black: String,
    red_won: bool,
    tournament: bool,
    /// Probabilité que le rouge gagne selon l'Elo d'avant la partie.
    elo_red: f64,
}

struct Fit {
    index: HashMap<String, usize>,
    strength: Vec<f64>,
    red_advantage: f64,
}

impl Fit {
    fn red_wins(&self, red: &str, black: &str) -> f64 {
        let s = |team: &str| self.index.get(team).map_or(0.0, |&i| self.strength[i]);
        sigmoid(s(red) - s(black) + self.red_advantage)
    }
}

/// Newton sur la log-vraisemblance pondérée ; le dernier paramètre est l'avantage du rouge.
fn fit(samples: &[Sample]) -> Fit {
    let mut index = HashMap::new();
    for game in samples {
        for team in [&game.red, &game.black] {
            let next = index.len();
            index.entry(team.clone()).or_insert(next);
        }
    }
    let c = index.len();
    let dim = c + 1;
    let decay = 0.5f64.powf(1.0 / HALF_LIFE);
    let mut weights = vec![1.0; samples.len()];
    for i in (0..samples.len().saturating_sub(1)).rev() {
        weights[i] = weights[i + 1] * decay;
    }
    let rows: Vec<_> = samples
        .iter()
        .zip(&weights)
        .map(|(g, &w)| (index[&g.red], index[&g.black], f64::from(u8::from(g.red_won)), w))
        .collect();
    let mut beta = vec![0.0; dim];
    for _ in 0..NEWTON_STEPS {
        let mut grad = vec![0.0; dim];
        let mut hess = vec![vec![0.0; dim]; dim];
        for &(r, b, y, w) in &rows {
            let p = sigmoid(beta[r] - beta[b] + beta[c]);
            let h = w * p * (1.0 - p);
            let x = [(r, 1.0), (b, -1.0), (c, 1.0)];
            for &(i, xi) in &x {
                grad[i] += w * (y - p) * xi;
                for &(j, xj) in &x {
                    hess[i][j] += h * xi * xj;
                }
            }
        }
        for t in 0..c {
            grad[t] -= RIDGE * beta[t];
            hess[t][t] += RIDGE;
        }
        hess[c][c] += 1e-9;
        let step = solve(hess, grad);
        let largest = step.iter().fold(0.0f64, |m, d| m.max(d.abs()));
        for (b, d) in beta.iter_mut().zip(step) {
            *b += d.clamp(-STEP_MAX, STEP_MAX);
        }
        if largest < 1e-8 {
            break;
        }
    }
    let red_advantage = beta.pop().unwrap_or(0.0);
    Fit {
        index,
        strength: beta,
        red_advantage,
    }
}

/// Élimination de Gauss avec pivot ; la matrice est définie positive (ridge).
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .unwrap_or(col);
        a.swap(col, pivot);
        b.swap(col, pivot);
        let top = a[col].clone();
        for row in col + 1..n {
            let f = a[row][col] / top[col];
            for (cell, t) in a[row][col..].iter_mut().zip(&top[col..]) {
                *cell -= f * t;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let tail: f64 = a[row][row + 1..].iter().zip(&x[row + 1..]).map(|(v, xv)| v * xv).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    x
}

#[derive(Serialize, Clone, Copy)]
struct Brier {
    model: f64,
    elo: f64,
    n: usize,
}

#[derive(Default)]
struct Tally {
    model: f64,
    elo: f64,
    n: usize,
}

impl Tally {
    fn add(&mut self, game: &Sample, fit: &Fit) {
        let y = f64::from(u8::from(game.red_won));
        self.model += (fit.red_wins(&game.red, &game.black) - y).powi(2);
        self.elo += (game.elo_red - y).powi(2);
        self.n += 1;
    }

    fn brier(&self) -> Option<Brier> {
        (self.n > 0).then(|| {
            let total = f64::from(u32::try_from(self.n).unwrap_or(u32::MAX));
            Brier {
                model: round4(self.model / total),
                elo: round4(self.elo / total),
                n: self.n,
            }
        })
    }
}

/// Chaque partie récente est notée par un modèle ajusté sur les parties qui la précèdent, réajusté
/// toutes les `REFIT_EVERY` parties comme en service ; l'Elo, lui, se met à jour partie par partie.
fn walk_forward(samples: &[Sample]) -> (Option<Brier>, Option<Brier>) {
    let holdout = (samples.len() / HOLDOUT_DIVISOR).min(HOLDOUT_MAX);
    let (mut all, mut tournament) = (Tally::default(), Tally::default());
    for start in (samples.len() - holdout..samples.len()).step_by(REFIT_EVERY as usize) {
        let trial = fit(&samples[..start]);
        let end = (start + REFIT_EVERY as usize).min(samples.len());
        for game in &samples[start..end] {
            all.add(game, &trial);
            if game.tournament {
                tournament.add(game, &trial);
            }
        }
    }
    (all.brier(), tournament.brier())
}

#[derive(Serialize, Deserialize, Clone)]
struct Point {
    ts: String,
    brier_model: f64,
    brier_elo: f64,
    n: usize,
}

/// Relu au démarrage pour que l'historique des métriques survive à un redémarrage.
#[derive(Deserialize)]
struct Stored {
    metrics: StoredMetrics,
}

#[derive(Deserialize)]
struct StoredMetrics {
    history: Vec<Point>,
}

#[derive(Serialize)]
struct TeamView {
    s: f64,
    n: u32,
}

#[derive(Serialize)]
struct Metrics<'a> {
    brier_model: Option<f64>,
    brier_elo: Option<f64>,
    holdout_n: usize,
    tournament: Option<Brier>,
    history: &'a [Point],
}

#[derive(Serialize)]
struct View<'a> {
    v: u32,
    ts: &'a str,
    usable: bool,
    red_advantage: f64,
    min_games: u32,
    teams: BTreeMap<&'a str, TeamView>,
    metrics: Metrics<'a>,
}

pub struct Model {
    path: PathBuf,
    samples: Vec<Sample>,
    ts: String,
    since_fit: u32,
    history: Vec<Point>,
}

impl Model {
    pub fn new(dir: &Path) -> Self {
        let path = dir.join("model.json");
        let history = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .map(|stored| stored.metrics.history)
            .unwrap_or_default();
        Self {
            path,
            samples: Vec::new(),
            ts: String::new(),
            since_fit: 0,
            history,
        }
    }

    /// `elo_red` : probabilité de victoire du rouge selon l'Elo d'avant la partie.
    /// Parties distantes écartées, ainsi que celles qui ne disent rien de la force de jeu.
    pub fn add(&mut self, game: &GameRecord, elo_red: f64) {
        let uninformative = matches!(
            game.result.reason,
            Reason::NoShow | Reason::Disconnect | Reason::Protocol
        );
        if game.red.remote || game.black.remote || uninformative {
            return;
        }
        self.ts.clone_from(&game.ts);
        self.since_fit += 1;
        self.samples.push(Sample {
            red: game.red.team.clone(),
            black: game.black.team.clone(),
            red_won: game.result.winner == Color::Red,
            tournament: game.tournament.is_some(),
            elo_red,
        });
    }

    pub fn publish_if_due(&mut self) -> io::Result<()> {
        if self.since_fit >= REFIT_EVERY {
            self.publish()?;
        }
        Ok(())
    }

    /// Valide au fil des parties les plus récentes, puis ajuste sur toutes. Écrit à côté puis renomme.
    pub fn publish(&mut self) -> io::Result<()> {
        self.since_fit = 0;
        let (overall, tournament) = walk_forward(&self.samples);
        let usable = overall.is_some_and(|b| b.n >= HOLDOUT_MIN && b.model < b.elo);
        if let Some(b) = overall {
            self.history.push(Point {
                ts: self.ts.clone(),
                brier_model: b.model,
                brier_elo: b.elo,
                n: b.n,
            });
            let excess = self.history.len().saturating_sub(HISTORY);
            self.history.drain(..excess);
        }
        let full = fit(&self.samples);
        let mut games: HashMap<&str, u32> = HashMap::new();
        for g in &self.samples {
            for team in [&g.red, &g.black] {
                *games.entry(team).or_default() += 1;
            }
        }
        let view = View {
            v: 1,
            ts: &self.ts,
            usable,
            red_advantage: round4(full.red_advantage),
            min_games: MIN_GAMES,
            teams: full
                .index
                .iter()
                .map(|(team, &i)| {
                    let view = TeamView {
                        s: round4(full.strength[i]),
                        n: games[team.as_str()],
                    };
                    (team.as_str(), view)
                })
                .collect(),
            metrics: Metrics {
                brier_model: overall.map(|b| b.model),
                brier_elo: overall.map(|b| b.elo),
                holdout_n: overall.map_or(0, |b| b.n),
                tournament,
                history: &self.history,
            },
        };
        write_atomic(&self.path, &serde_json::to_vec(&view)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(red: &str, black: &str, red_won: bool, elo_red: f64) -> Sample {
        Sample {
            red: red.into(),
            black: black.into(),
            red_won,
            tournament: false,
            elo_red,
        }
    }

    fn strength(fit: &Fit, team: &str) -> f64 {
        fit.strength[fit.index[team]]
    }

    /// Quatre équipes, la plus petite l'emporte sauf une partie sur cinq.
    fn ranked_games(count: usize, elo_red: f64) -> Vec<Sample> {
        let teams = ["a", "b", "c", "d"];
        (0..count)
            .map(|i| {
                let (r, b) = (i % 4, (i + 1 + i / 4 % 3) % 4);
                let stronger_red = r < b;
                sample(teams[r], teams[b], stronger_red == (i % 5 != 0), elo_red)
            })
            .collect()
    }

    fn read(dir: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(dir.join("model.json")).unwrap()).unwrap()
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("isolation-model-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fit_orders_teams_by_strength() {
        let fit = fit(&ranked_games(400, 0.5));
        let order: Vec<f64> = ["a", "b", "c", "d"].iter().map(|t| strength(&fit, t)).collect();
        assert!(order.windows(2).all(|w| w[0] > w[1]), "{order:?}");
    }

    #[test]
    fn red_advantage_is_positive_when_red_wins_between_equals() {
        let games: Vec<_> = (0..200)
            .map(|i| {
                let (red, black) = if i % 2 == 0 { ("a", "b") } else { ("b", "a") };
                sample(red, black, i / 2 % 10 < 7, 0.5)
            })
            .collect();
        let fit = fit(&games);
        assert!(fit.red_advantage > 0.5, "{}", fit.red_advantage);
        assert!((strength(&fit, "a") - strength(&fit, "b")).abs() < 0.2);
    }

    #[test]
    fn a_team_that_never_loses_keeps_a_finite_strength() {
        let games: Vec<_> = (0..40)
            .map(|i| {
                if i % 2 == 0 {
                    sample("a", "b", true, 0.5)
                } else {
                    sample("b", "a", false, 0.5)
                }
            })
            .collect();
        let fit = fit(&games);
        let a = strength(&fit, "a");
        assert!(a > 0.0 && a < 5.0, "{a}");
    }

    #[test]
    fn unknown_team_counts_as_average() {
        let fit = fit(&ranked_games(100, 0.5));
        let p = fit.red_wins("nouvelle", "inconnue");
        assert!((p - sigmoid(fit.red_advantage)).abs() < 1e-12);
    }

    #[test]
    fn model_is_published_only_when_it_beats_the_elo() {
        let dir = temp("gate");
        let mut model = Model::new(&dir);
        model.samples = ranked_games(400, 0.5);
        model.publish().unwrap();
        let json = read(&dir);
        assert_eq!(json["usable"], true);
        assert!(json["metrics"]["brier_model"].as_f64().unwrap() < json["metrics"]["brier_elo"].as_f64().unwrap());
        assert_eq!(json["teams"]["a"]["n"], 200);

        let mut oracle = Model::new(&dir);
        oracle.samples = ranked_games(400, 0.5)
            .into_iter()
            .map(|g| Sample {
                elo_red: f64::from(u8::from(g.red_won)),
                ..g
            })
            .collect();
        oracle.publish().unwrap();
        assert_eq!(read(&dir)["usable"], false);
        assert_eq!(read(&dir)["metrics"]["history"].as_array().unwrap().len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn small_holdout_is_not_usable() {
        let dir = temp("small");
        let mut model = Model::new(&dir);
        model.samples = ranked_games(40, 0.5);
        model.publish().unwrap();
        assert_eq!(read(&dir)["usable"], false);
        assert!(!dir.join("model.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_history_publishes_an_unusable_model() {
        let dir = temp("empty");
        Model::new(&dir).publish().unwrap();
        let json = read(&dir);
        assert_eq!(json["usable"], false);
        assert_eq!(json["teams"].as_object().unwrap().len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
