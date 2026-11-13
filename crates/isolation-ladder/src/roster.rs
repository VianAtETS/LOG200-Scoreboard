use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::command::CommandTemplate;
use crate::config::{Config, TeamConfig};
use crate::placement::{PairClass, Placement};
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
pub fn pair_class(placement: &Placement, a: &Team, b: &Team) -> PairClass {
    placement.class(a.multithread || b.multithread)
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
        let placement = Placement::new(&cfg);
        assert_eq!(pair_class(&placement, &single, &single), PairClass::Heavy);
        assert_eq!(pair_class(&placement, &single, &multi), PairClass::Multithread);
        assert_eq!(pair_class(&placement, &multi, &single), PairClass::Multithread);
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

        let ordinary_only = Placement::new(&MINIMAL.parse().unwrap());
        assert_eq!(pair_class(&ordinary_only, &multi, &single), PairClass::Heavy);
        let multithread_only: Config = format!("{MINIMAL}[[slot]]\nmultithread = true\n").parse().unwrap();
        let multithread_only = Placement::new(&multithread_only);
        assert_eq!(pair_class(&multithread_only, &single, &single), PairClass::Multithread);
    }

    #[test]
    fn registered_team_named_like_a_fixed_team_is_reported_once() {
        let dir = std::env::temp_dir().join(format!("isolation-roster-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n\
             [submit]\nteams = {teams:?}\njars = {jars:?}\n\
             [[team]]\nid = \"niveau-1\"\ncmd = [\"bot\"]\n",
            teams = dir.join("teams.toml"),
            jars = dir.join("jars"),
        );
        let cfg: Config = toml.parse().unwrap();
        let submit = cfg.submit.clone().unwrap();
        for id in ["niveau-1", "eq01", submit::SMOKE_OPPONENT] {
            let id: TeamId = id.parse().unwrap();
            submit::add_team(&submit.teams, id.clone()).unwrap();
            let jar = submit.current_jar(&id);
            std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
            std::fs::write(jar, "").unwrap();
        }
        let mut warned = HashSet::new();
        for _ in 0..2 {
            let ids: Vec<String> = roster(&cfg, None, &mut warned).into_iter().map(|t| t.id).collect();
            assert_eq!(ids, ["niveau-1", "eq01", submit::SMOKE_OPPONENT]);
        }
        assert_eq!(
            warned,
            HashSet::from(["niveau-1".into(), submit::SMOKE_OPPONENT.into()])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
