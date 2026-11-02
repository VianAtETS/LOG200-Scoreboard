//! Soumission des jars : un jar qui passe le smoke test remplace atomiquement le jar actif de
//! l'équipe. Les smoke tests se jouent un à la fois, après la partie en cours du ladder.

mod auth;
mod handler;
mod manifest;
mod smoke;

pub use auth::{DEFAULT_TEAMS, RegistryError, add_team, bearer, find_team, load_teams};
pub use handler::{Submissions, SubmitError};
pub use smoke::SMOKE_OPPONENT;

use crate::config::Config;
use crate::roster::Team;

/// Équipes inscrites dont un jar a été accepté.
pub fn active_teams(cfg: &Config) -> Result<Vec<Team>, RegistryError> {
    let Some(submit) = &cfg.submit else {
        return Ok(Vec::new());
    };
    Ok(auth::load_teams(&submit.teams)?
        .into_iter()
        .filter_map(|registered| {
            let jar = submit.current_jar(&registered.id);
            jar.exists()
                .then(|| Team::with_jar(cfg, &registered.id, &jar, manifest::multithread_file(&jar)))
        })
        .collect())
}

#[cfg(test)]
pub mod test_support {
    use std::path::PathBuf;

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("isolation-submit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
