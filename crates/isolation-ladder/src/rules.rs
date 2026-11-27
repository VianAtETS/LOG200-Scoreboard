//! Ce que le ladder demande aux règles du jeu, sans les connaître : il lance l'exécutable de
//! l'arbitre (`rules` dans ladder.toml) avec ses sous-commandes `opening` et `check`
//! (docs/formats.md, "Sous-commandes"). Hors bac à sable : elles ne parlent à aucun joueur.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

use game_kit::Change;
use game_kit::record::GameResult;
use serde::Deserialize;
use thiserror::Error;

use crate::command::CommandTemplate;
use crate::config::Config;

/// Les ouvertures d'une paire, ou de deux manches d'un match, sont la même : même graine, même
/// ouverture (l'arbitre la tire de façon reproductible).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Opening {
    pub plies: usize,
    pub seed: u64,
}

#[cfg(test)]
impl Opening {
    /// Pas d'ouverture : les joueurs partent de la position initiale.
    pub const NONE: Self = Self { plies: 0, seed: 0 };
}

#[derive(Debug, Error)]
pub enum RulesError {
    /// Le résultat contredit les règles : coup illégal, gagnant incohérent...
    #[error("{0}")]
    Rejected(String),
    /// Les règles n'ont pas pu répondre : exécutable absent, sortie illisible.
    #[error("règles du jeu ({program}) : {message}")]
    Failed { program: String, message: String },
}

pub trait Rules: Send + Sync {
    /// Les coups de l'ouverture, en notation canonique.
    fn opening(&self, opening: Opening) -> Result<Vec<String>, RulesError>;

    /// Le résultat rejoué : ses effets, demi-coup par demi-coup (ouverture comprise), s'il est
    /// cohérent avec les règles.
    fn check(&self, result: &GameResult) -> Result<Vec<Vec<Change>>, RulesError>;
}

/// Les règles de `rules` dans ladder.toml. Dans les tests du ladder, sans `rules`, celles
/// d'Isolation en direct (`Isolation`), pour les arbitres factices.
pub fn of(cfg: &Config) -> Arc<dyn Rules> {
    match &cfg.rules {
        Some(cmd) => Arc::new(Referee::new(cmd.clone())),
        #[cfg(test)]
        None => Arc::new(Isolation),
        #[cfg(not(test))]
        None => Arc::new(Referee::new(CommandTemplate::new(["isolation-referee"]))),
    }
}

/// Les règles de l'exécutable d'un arbitre construit avec `game_kit::referee` (ou compatible).
pub struct Referee {
    cmd: CommandTemplate,
}

impl Referee {
    pub const fn new(cmd: CommandTemplate) -> Self {
        Self { cmd }
    }

    fn program(&self) -> String {
        self.cmd.args().first().cloned().unwrap_or_default()
    }

    fn failed(&self, message: &dyn std::fmt::Display) -> RulesError {
        RulesError::Failed {
            program: self.program(),
            message: message.to_string(),
        }
    }

    /// Lance une sous-commande, avec `input` sur son entrée standard ; rend sa sortie standard.
    fn run(&self, args: &[String], input: Option<&str>) -> Result<String, RulesError> {
        let (program, base) = self
            .cmd
            .args()
            .split_first()
            .ok_or_else(|| self.failed(&"commande vide"))?;
        let mut child = Command::new(program)
            .args(base)
            .args(args)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| self.failed(&e))?;
        if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(text.as_bytes()).map_err(|e| self.failed(&e))?;
        }
        let output = child.wait_with_output().map_err(|e| self.failed(&e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(self.failed(&format!("{} : {}", output.status, stderr.trim())));
        }
        String::from_utf8(output.stdout).map_err(|e| self.failed(&e))
    }
}

/// Réponse de `check` pour un résultat.
#[derive(Deserialize)]
struct Checked {
    ok: bool,
    #[serde(default)]
    changes: Vec<Vec<Change>>,
    #[serde(default)]
    error: String,
}

impl Rules for Referee {
    fn opening(&self, opening: Opening) -> Result<Vec<String>, RulesError> {
        if opening.plies == 0 {
            return Ok(Vec::new());
        }
        let args = [
            "opening".to_string(),
            "--plies".into(),
            opening.plies.to_string(),
            "--seed".into(),
            opening.seed.to_string(),
        ];
        let out = self.run(&args, None)?;
        let moves: Vec<String> = out
            .trim()
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(String::from)
            .collect();
        if moves.len() > opening.plies {
            return Err(self.failed(&format!("ouverture de {} demi-coups : {out:?}", moves.len())));
        }
        Ok(moves)
    }

    fn check(&self, result: &GameResult) -> Result<Vec<Vec<Change>>, RulesError> {
        let line = serde_json::to_string(result).map_err(|e| self.failed(&e))?;
        let out = self.run(&["check".to_string()], Some(&format!("{line}\n")))?;
        let checked: Checked = out
            .lines()
            .next()
            .ok_or_else(|| self.failed(&"aucune réponse de check"))
            .and_then(|l| serde_json::from_str(l).map_err(|e| self.failed(&format!("réponse illisible : {e}"))))?;
        if checked.ok {
            Ok(checked.changes)
        } else {
            Err(RulesError::Rejected(checked.error))
        }
    }
}

/// Les règles d'Isolation en direct, sans processus : celles des tests du ladder, dont les
/// arbitres factices jouent des parties d'Isolation.
#[cfg(test)]
pub struct Isolation;

#[cfg(test)]
impl Rules for Isolation {
    fn opening(&self, opening: Opening) -> Result<Vec<String>, RulesError> {
        use game_kit::Game;
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(opening.seed);
        let moves = isolation_core::Isolation.random_opening(opening.plies, &mut rng);
        Ok(moves.iter().map(ToString::to_string).collect())
    }

    fn check(&self, result: &GameResult) -> Result<Vec<Vec<Change>>, RulesError> {
        result
            .replay(&isolation_core::Isolation)
            .map(|replay| replay.changes)
            .map_err(|e| RulesError::Rejected(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use game_kit::ByColor;
    use game_kit::record::{RECORD_VERSION, Reason};

    use super::*;

    fn result(moves: &[&str]) -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            game: None,
            game_version: None,
            time_ms: 500,
            opening: vec![],
            moves: moves.iter().map(ToString::to_string).collect(),
            winner: Some(game_kit::Color::Red),
            reason: Reason::Timeout,
            invalid: ByColor::default(),
            duration_ms: 1,
            move_ms: vec![],
        }
    }

    fn sh(script: &str) -> Referee {
        Referee::new(CommandTemplate::new(["sh", "-c", script, "rules"]))
    }

    #[test]
    fn the_opening_comes_from_the_referee_with_its_plies_and_seed() {
        let rules =
            sh(r#"echo "$1 $2 $3 $4" >&2; [ "$*" = "opening --plies 2 --seed 7" ] && echo "G1-G4-F9,F12-F10-G7""#);
        let opening = rules.opening(Opening { plies: 2, seed: 7 }).unwrap();
        assert_eq!(opening, ["G1-G4-F9", "F12-F10-G7"]);
        assert_eq!(
            sh("exit 1").opening(Opening::NONE).unwrap(),
            Vec::<String>::new(),
            "rien à lancer"
        );
    }

    #[test]
    fn a_failing_referee_is_a_failure_not_a_rejection() {
        let error = sh("echo boom >&2; exit 2")
            .opening(Opening { plies: 2, seed: 1 })
            .unwrap_err();
        assert!(
            matches!(&error, RulesError::Failed { message, .. } if message.contains("boom")),
            "{error}"
        );
        let missing = Referee::new(CommandTemplate::new(["/nonexistent/referee"]));
        assert!(matches!(missing.check(&result(&[])), Err(RulesError::Failed { .. })));
    }

    #[test]
    fn check_reads_one_answer_per_result() {
        let ok = sh(r#"read line; echo '{"ok":true,"changes":[[{"cell":138,"code":0,"role":"from"}]]}'"#);
        let changes = ok.check(&result(&["G1-G4-F9"])).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0][0].cell, 138);
        let rejected = sh(r#"read line; echo '{"ok":false,"error":"coup 1 illégal : G1-G1-G1"}'"#);
        assert!(
            matches!(rejected.check(&result(&["G1-G1-G1"])), Err(RulesError::Rejected(e)) if e.contains("illégal"))
        );
    }

    #[test]
    fn the_test_rules_are_isolation() {
        let opening = Isolation.opening(Opening { plies: 2, seed: 42 }).unwrap();
        assert_eq!(opening.len(), 2);
        assert_eq!(Isolation.opening(Opening { plies: 2, seed: 42 }).unwrap(), opening);
        assert!(Isolation.check(&result(&["G1-G1-G1"])).is_err());
    }
}
