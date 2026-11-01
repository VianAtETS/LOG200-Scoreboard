use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::command::CommandTemplate;
use crate::config::{Config, TeamConfig};
use crate::remote::{Registry, Seat};
use crate::submit;
use crate::team_id::TeamId;

#[derive(Clone)]
pub struct Team {
    pub id: String,
    pub cmd: CommandTemplate,
    pub build: Option<PathBuf>,
    pub anchor: bool,
    pub multithread: bool,
    /// Équipe connectée depuis sa machine : elle joue à la place de `cmd`.
    pub remote: Option<Seat>,
}

impl From<&TeamConfig> for Team {
    fn from(team: &TeamConfig) -> Self {
        Self {
            id: team.id.clone(),
            cmd: team.cmd.clone(),
            build: team.build.clone(),
            anchor: team.anchor,
            multithread: team.multithread,
            remote: None,
        }
    }
}

impl Team {
    pub fn with_jar(cfg: &Config, id: &TeamId, jar: &Path, multithread: bool) -> Self {
        Self {
            id: id.to_string(),
            cmd: cfg.team_cmd.with_jar(&jar.to_string_lossy()),
            build: Some(jar.to_path_buf()),
            anchor: false,
            multithread,
            remote: None,
        }
    }

    pub fn build_file(&self) -> Option<PathBuf> {
        self.build.clone().or_else(|| self.cmd.jar().map(PathBuf::from))
    }

    /// Adversaire de référence, sans soumission.
    pub fn is_bot(&self) -> bool {
        self.remote.is_none() && self.build_file().is_none()
    }
}

/// Multithread dès qu'une des équipes l'est.
pub fn multithread_game(cfg: &Config, a: &Team, b: &Team) -> bool {
    cfg.slot_kind(a.multithread || b.multithread)
}

/// Équipes de `[[team]]`, puis les équipes inscrites qui n'y figurent pas. Une équipe connectée en
/// distant joue à la place de son jar.
pub fn roster(cfg: &Config, remote: Option<&Arc<Registry>>, warned: &mut HashSet<String>) -> Vec<Team> {
    let mut teams: Vec<Team> = cfg.teams.iter().map(Team::from).collect();
    let mut registered = submit::active_teams(cfg).unwrap_or_else(|e| {
        eprintln!("équipes inscrites ignorées : {e}");
        Vec::new()
    });
    for online in remote.map(Registry::online).unwrap_or_default() {
        registered.retain(|team| team.id != online.id);
        registered.push(online);
    }
    for team in registered {
        let fixed = teams.iter().any(|t| t.id == team.id);
        if (fixed || team.id == submit::SMOKE_OPPONENT) && warned.insert(team.id.clone()) {
            if fixed {
                eprintln!(
                    "équipe inscrite {} ignorée : une équipe fixe de la config a le même id",
                    team.id
                );
            } else {
                eprintln!("équipe inscrite {} : même id que l'adversaire du smoke test", team.id);
            }
        }
        if !fixed {
            teams.push(team);
        }
    }
    teams
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Vars;
    use crate::config::tests::MINIMAL;

    #[test]
    fn multithread_slots_host_every_game_of_a_multithread_team() {
        let text = format!("{MINIMAL}[[slot]]\ncpus = \"1\"\n[[slot]]\nmultithread = true\ncpus = \"2\"\n");
        let cfg: Config = text.parse().unwrap();
        assert_eq!((cfg.slots_of(false), cfg.slots_of(true)), (1, 1));
        let id: TeamId = "eq07".parse().unwrap();
        let jar = Path::new("current.jar");
        let (single, multi) = (
            Team::with_jar(&cfg, &id, jar, false),
            Team::with_jar(&cfg, &id, jar, true),
        );
        assert!(!multithread_game(&cfg, &single, &single));
        assert!(multithread_game(&cfg, &single, &multi));
        assert!(multithread_game(&cfg, &multi, &single));
        let vars = Vars {
            cfg: &cfg,
            slot: 1,
            opening: String::new(),
            game: "g1",
            time_ms: cfg.time_ms,
        };
        let command = vars.command(&cfg.referee, None).unwrap();
        let keys: Vec<_> = command
            .get_envs()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert_eq!(keys, ["SANDBOX_CPUS"]);

        let ordinary_only: Config = MINIMAL.parse().unwrap();
        assert!(!multithread_game(&ordinary_only, &multi, &single));
        let multithread_only: Config = format!("{MINIMAL}[[slot]]\nmultithread = true\n").parse().unwrap();
        assert!(multithread_game(&multithread_only, &single, &single));
    }
}
