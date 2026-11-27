//! Ce que le ladder demande aux règles du jeu, sans les connaître : il lance l'exécutable de
//! l'arbitre (`rules` dans ladder.toml) avec ses sous-commandes `describe`, `opening` et `check`
//! (docs/formats.md, "Sous-commandes"). Hors bac à sable : elles ne parlent à aucun joueur.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use game_kit::record::GameResult;
use game_kit::{Change, Descriptor};
use serde::Deserialize;
use thiserror::Error;

use crate::command::CommandTemplate;
use crate::config::Config;
use crate::db::Db;
use crate::store::write_atomic;

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
    /// Le descripteur du jeu.
    fn describe(&self) -> Result<Descriptor, RulesError>;

    /// Les coups de l'ouverture, en notation canonique.
    fn opening(&self, opening: Opening) -> Result<Vec<String>, RulesError>;

    /// Chaque résultat rejoué, dans l'ordre : ses effets demi-coup par demi-coup (ouverture
    /// comprise), ou pourquoi les règles le refusent.
    fn check_many(&self, results: &[&GameResult]) -> Result<Vec<Checked>, RulesError>;

    /// Un seul résultat, comme `check_many`.
    fn check(&self, result: &GameResult) -> Result<Vec<Vec<Change>>, RulesError> {
        match self.check_many(&[result])?.pop() {
            Some(checked) => checked.map_err(RulesError::Rejected),
            None => Err(RulesError::Failed {
                program: String::new(),
                message: "aucune réponse de check".into(),
            }),
        }
    }
}

/// Les effets d'un résultat cohérent, ou la raison du refus.
pub type Checked = Result<Vec<Vec<Change>>, String>;

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
    /// Le descripteur ne change pas tant que l'exécutable tourne : demandé une fois.
    descriptor: OnceLock<Descriptor>,
}

impl Referee {
    pub const fn new(cmd: CommandTemplate) -> Self {
        Self {
            cmd,
            descriptor: OnceLock::new(),
        }
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
    /// L'entrée est écrite par un autre thread : une longue sortie ne bloque pas une longue entrée.
    fn run(&self, args: &[String], input: Option<String>) -> Result<String, RulesError> {
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
        let writer = match (input, child.stdin.take()) {
            (Some(text), Some(mut stdin)) => Some(std::thread::spawn(move || stdin.write_all(text.as_bytes()))),
            _ => None,
        };
        let output = child.wait_with_output().map_err(|e| self.failed(&e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(self.failed(&format!("{} : {}", output.status, stderr.trim())));
        }
        if let Some(writer) = writer {
            writer
                .join()
                .map_err(|_| self.failed(&"écriture de l'entrée interrompue"))?
                .map_err(|e| self.failed(&e))?;
        }
        String::from_utf8(output.stdout).map_err(|e| self.failed(&e))
    }
}

/// Réponse de `check` pour un résultat.
#[derive(Deserialize)]
struct Answer {
    ok: bool,
    #[serde(default)]
    changes: Vec<Vec<Change>>,
    #[serde(default)]
    error: String,
}

impl Rules for Referee {
    fn describe(&self) -> Result<Descriptor, RulesError> {
        if let Some(descriptor) = self.descriptor.get() {
            return Ok(descriptor.clone());
        }
        let out = self.run(&["describe".to_string()], None)?;
        let descriptor: Descriptor =
            serde_json::from_str(out.trim()).map_err(|e| self.failed(&format!("descripteur illisible : {e}")))?;
        let problems = descriptor.problems();
        if !problems.is_empty() {
            return Err(self.failed(&format!("descripteur incohérent : {}", problems.join(" ; "))));
        }
        Ok(self.descriptor.get_or_init(|| descriptor).clone())
    }

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

    fn check_many(&self, results: &[&GameResult]) -> Result<Vec<Checked>, RulesError> {
        if results.is_empty() {
            return Ok(Vec::new());
        }
        let mut input = String::new();
        for result in results {
            input.push_str(&serde_json::to_string(result).map_err(|e| self.failed(&e))?);
            input.push('\n');
        }
        let out = self.run(&["check".to_string()], Some(input))?;
        let answers = out
            .lines()
            .map(|line| {
                serde_json::from_str::<Answer>(line).map_err(|e| self.failed(&format!("réponse illisible : {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if answers.len() != results.len() {
            return Err(self.failed(&format!("{} réponses pour {} résultats", answers.len(), results.len())));
        }
        Ok(answers
            .into_iter()
            .map(|a| if a.ok { Ok(a.changes) } else { Err(a.error) })
            .collect())
    }
}

/// Parties rejouées par appel à `check` pendant le remplissage : un processus par lot.
const BACKFILL_BATCH: usize = 200;

/// Au démarrage : le jeu de cette instance. Son descripteur est gardé dans la base (une fois par
/// version, jamais remplacé) et publié pour la page dans `data/game.json`.
pub fn adopt(rules: &dyn Rules, db: &Db, data_dir: &Path) -> anyhow::Result<Descriptor> {
    let descriptor = rules
        .describe()
        .context("descripteur du jeu (`rules` dans ladder.toml)")?;
    if !db.put_descriptor(&descriptor)? {
        let stored = db.descriptor(&descriptor.game, descriptor.version)?;
        if stored.as_ref() != Some(&descriptor) {
            eprintln!(
                "ATTENTION : le descripteur de {} v{} a changé sans changer de version. Les parties \
                 gardent celui de la base ; monter la version du jeu pour changer ses règles ou son affichage.",
                descriptor.game, descriptor.version
            );
        }
    }
    let json = serde_json::to_vec(&descriptor)?;
    write_atomic(&data_dir.join("game.json"), &json).with_context(|| data_dir.display().to_string())?;
    eprintln!("jeu : {} (v{})", descriptor.name, descriptor.version);
    Ok(descriptor)
}

/// Les effets des parties enregistrées sans (avant la v3 de la base, ou une écriture ratée), rejoués
/// par `check`. Une partie refusée garde `NULL` et sera retentée au prochain démarrage. Rend le
/// nombre de parties remplies et refusées.
pub fn backfill_changes(rules: &dyn Rules, db: &Db) -> anyhow::Result<(usize, usize)> {
    let (mut filled, mut refused, mut after) = (0, 0, 0);
    loop {
        let batch = db.missing_changes(after, BACKFILL_BATCH)?;
        let Some(&(last, _)) = batch.last() else {
            break;
        };
        let results: Vec<&GameResult> = batch.iter().map(|(_, game)| &game.result).collect();
        for ((seq, game), checked) in batch.iter().zip(rules.check_many(&results)?) {
            match checked {
                Ok(changes) => {
                    db.set_changes(*seq, &changes)?;
                    filled += 1;
                }
                Err(why) => {
                    eprintln!("   effets de {} : partie refusée par les règles ({why})", game.id);
                    refused += 1;
                }
            }
        }
        after = last;
    }
    if filled + refused > 0 {
        eprintln!("effets rejoués : {filled} parties, {refused} refusées");
    }
    Ok((filled, refused))
}

/// Les règles d'Isolation en direct, sans processus : celles des tests du ladder, dont les
/// arbitres factices jouent des parties d'Isolation.
#[cfg(test)]
pub struct Isolation;

#[cfg(test)]
impl Rules for Isolation {
    fn describe(&self) -> Result<Descriptor, RulesError> {
        use game_kit::Game;
        Ok(isolation::Isolation.descriptor())
    }

    fn opening(&self, opening: Opening) -> Result<Vec<String>, RulesError> {
        use game_kit::Game;
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(opening.seed);
        let moves = isolation::Isolation.random_opening(opening.plies, &mut rng);
        Ok(moves.iter().map(ToString::to_string).collect())
    }

    fn check_many(&self, results: &[&GameResult]) -> Result<Vec<Checked>, RulesError> {
        Ok(results
            .iter()
            .map(|result| {
                result
                    .replay(&isolation::Isolation)
                    .map(|replay| replay.changes)
                    .map_err(|e| e.to_string())
            })
            .collect())
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
    fn adopt_keeps_the_first_descriptor_of_a_version_and_publishes_it() {
        let dir = crate::submit::test_support::temp_dir("rules-adopt");
        let db = crate::db::tests::memory();
        let descriptor = adopt(&Isolation, &db, &dir).unwrap();
        assert_eq!((descriptor.game.as_str(), descriptor.version), ("isolation", 1));
        let published: Descriptor = serde_json::from_slice(&std::fs::read(dir.join("game.json")).unwrap()).unwrap();
        assert_eq!(published, descriptor);
        assert_eq!(db.descriptor("isolation", 1).unwrap(), Some(descriptor.clone()));

        let mut changed = descriptor.clone();
        changed.name = "Isola".into();
        assert!(!db.put_descriptor(&changed).unwrap(), "jamais remplacé");
        assert_eq!(db.descriptor("isolation", 1).unwrap(), Some(descriptor));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn backfill_fills_what_the_rules_accept_and_leaves_the_rest_for_later() {
        let db = crate::db::tests::memory();
        let mut absent = crate::db::tests::game("g1", "a", "b", game_kit::Color::Red);
        absent.result.reason = Reason::NoShow;
        absent.result.opening.clear();
        absent.result.moves.clear();
        // Trois coups légaux, mais pas une fin `no_moves` : refusée.
        let wrong = crate::db::tests::game("g2", "b", "a", game_kit::Color::Red);
        for game in [&absent, &wrong] {
            db.insert_game(game, None, &[]).unwrap();
        }
        assert_eq!(backfill_changes(&Isolation, &db).unwrap(), (1, 1));
        assert_eq!(db.changes_json("g1").unwrap().as_deref(), Some("[]"));
        assert_eq!(db.changes_json("g2").unwrap(), None);
        assert_eq!(
            backfill_changes(&Isolation, &db).unwrap(),
            (0, 1),
            "la refusée est retentée"
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
