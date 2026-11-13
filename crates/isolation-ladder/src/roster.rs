use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use isolation_core::digest::short_hash;

use crate::command::CommandTemplate;
use crate::config::{Config, NodesConfig, TeamConfig};
use crate::placement::{PairClass, Placement};
use crate::remote::{Registry, Seat};
use crate::standings::Measured;
use crate::submit;
use crate::team_id::TeamId;

#[derive(Clone)]
pub struct Team {
    pub id: String,
    pub cmd: CommandTemplate,
    pub build: Option<PathBuf>,
    pub anchor: bool,
    pub multithread: bool,
    /// Déclaré portable dans la config (bots dont la force ne dépend pas de la machine).
    pub portable: bool,
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
            portable: team.portable,
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
            portable: false,
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

/// Multithread dès qu'une des équipes l'est ; portable si les deux sont dans `portable`.
pub fn pair_class(placement: &Placement, portable: &HashSet<String>, a: &Team, b: &Team) -> PairClass {
    let multithread = a.multithread || b.multithread;
    if !multithread && portable.contains(&a.id) && portable.contains(&b.id) {
        PairClass::Portable
    } else {
        placement.class(multithread)
    }
}

/// Peut jouer sur un nœud sans que sa force en dépende : bot déclaré portable, ou build actif
/// (`build`) assez rapide, mesuré ici. `pace` donne le rythme d'un build de l'équipe.
pub fn is_portable(
    team: &Team,
    nodes: &NodesConfig,
    build: Option<&str>,
    pace: impl Fn(&str) -> Option<Measured>,
) -> bool {
    if team.multithread || team.remote.is_some() {
        return false;
    }
    if team.portable {
        return true;
    }
    build
        .and_then(pace)
        .is_some_and(|m| m.p99 < nodes.portable_p99_ms && m.count >= nodes.portable_min_moves)
}

/// Les équipes portables du roster ; aucune sans `[nodes]`.
pub fn portable_teams(
    cfg: &Config,
    teams: &[Team],
    builds: &mut Builds,
    pace: impl Fn(&str, &str) -> Option<Measured>,
) -> HashSet<String> {
    let Some(nodes) = &cfg.nodes else {
        return HashSet::new();
    };
    teams
        .iter()
        .filter(|team| {
            let build = team.build_file().and_then(|path| builds.hash(&path));
            is_portable(team, nodes, build.as_deref(), |b| pace(&team.id, b))
        })
        .map(|team| team.id.clone())
        .collect()
}

/// Hash court des jars actifs, relu seulement quand le fichier change (taille ou date).
#[derive(Default)]
pub struct Builds(HashMap<PathBuf, (u64, SystemTime, String)>);

impl Builds {
    pub fn hash(&mut self, path: &Path) -> Option<String> {
        let meta = fs::metadata(path).ok()?;
        let stamp = (meta.len(), meta.modified().ok()?);
        if let Some((len, modified, hash)) = self.0.get(path)
            && (*len, *modified) == stamp
        {
            return Some(hash.clone());
        }
        let hash = short_hash(&fs::read(path).ok()?);
        self.0.insert(path.to_path_buf(), (stamp.0, stamp.1, hash.clone()));
        Some(hash)
    }
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
        let none = HashSet::new();
        assert_eq!(pair_class(&placement, &none, &single, &single), PairClass::Heavy);
        assert_eq!(pair_class(&placement, &none, &single, &multi), PairClass::Multithread);
        assert_eq!(pair_class(&placement, &none, &multi, &single), PairClass::Multithread);
        let vars = Vars {
            cfg: &cfg.exec,
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
        assert_eq!(pair_class(&ordinary_only, &none, &multi, &single), PairClass::Heavy);
        let multithread_only: Config = format!("{MINIMAL}[[slot]]\nmultithread = true\n").parse().unwrap();
        let multithread_only = Placement::new(&multithread_only);
        assert_eq!(
            pair_class(&multithread_only, &none, &single, &single),
            PairClass::Multithread
        );
    }

    fn nodes() -> NodesConfig {
        format!("{MINIMAL}[nodes]\n").parse::<Config>().unwrap().nodes.unwrap()
    }

    fn fixed(id: &str, portable: bool) -> Team {
        Team {
            portable,
            ..Team::from(&TeamConfig {
                id: id.into(),
                cmd: CommandTemplate::new(["bot"]),
                build: None,
                anchor: false,
                multithread: false,
                portable,
            })
        }
    }

    fn measured(p99: u64, count: u64) -> impl Fn(&str) -> Option<Measured> {
        move |_| Some(Measured { p99, count })
    }

    #[test]
    fn build_hashes_follow_the_jar_and_are_cached_until_it_changes() {
        let dir = std::env::temp_dir().join(format!("isolation-builds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("current.jar");
        let mut builds = Builds::default();
        assert_eq!(builds.hash(&jar), None, "pas de jar");
        fs::write(&jar, "v1").unwrap();
        let v1 = builds.hash(&jar).unwrap();
        assert_eq!(v1, short_hash(b"v1"));
        assert_eq!(builds.hash(&jar).unwrap(), v1, "relu depuis le cache");
        fs::write(&jar, "v2, plus long").unwrap();
        assert_eq!(
            builds.hash(&jar).unwrap(),
            short_hash(b"v2, plus long"),
            "nouveau jar, nouveau hash"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bots_are_portable_only_when_flagged() {
        let none = |_: &str| None;
        assert!(is_portable(&fixed("aleatoire", true), &nodes(), None, none));
        assert!(!is_portable(&fixed("niveau-4", false), &nodes(), None, none));
    }

    #[test]
    fn new_build_is_not_portable_until_measured_on_the_main_server() {
        let cfg: Config = MINIMAL.parse().unwrap();
        let team = Team::with_jar(&cfg, &"eq07".parse().unwrap(), Path::new("j.jar"), false);
        let nodes = nodes();
        assert!(!is_portable(&team, &nodes, Some("v2"), |_| None));
        assert!(!is_portable(&team, &nodes, Some("v2"), measured(99, 199)));
        assert!(is_portable(&team, &nodes, Some("v2"), measured(99, 200)));
        assert!(!is_portable(&team, &nodes, Some("v2"), measured(100, 500)));
        assert!(!is_portable(&team, &nodes, None, measured(10, 500)), "build illisible");
        let asked = std::cell::RefCell::new(String::new());
        is_portable(&team, &nodes, Some("v2"), |build| {
            asked.replace(build.into());
            None
        });
        assert_eq!(
            asked.into_inner(),
            "v2",
            "le rythme du build actif, pas du dernier joué"
        );
    }

    #[test]
    fn multithread_and_remote_teams_are_never_portable() {
        let mut multi = fixed("niveau-5", true);
        multi.multithread = true;
        assert!(!is_portable(&multi, &nodes(), None, |_| None));
        let cfg: Config = MINIMAL.parse().unwrap();
        let placement = Placement::new(&cfg);
        let portable = HashSet::from(["niveau-5".to_string(), "aleatoire".to_string()]);
        assert_eq!(
            pair_class(&placement, &portable, &multi, &fixed("aleatoire", true)),
            PairClass::Heavy,
            "sans slot multithread, une paire multithread est ordinaire, jamais portable"
        );
    }

    #[test]
    fn a_pair_is_portable_only_if_both_players_are() {
        let cfg: Config = MINIMAL.parse().unwrap();
        let placement = Placement::new(&cfg);
        let portable = HashSet::from(["aleatoire".to_string(), "niveau-1".to_string()]);
        let (a, b, c) = (fixed("aleatoire", true), fixed("niveau-1", true), fixed("eq07", false));
        assert_eq!(pair_class(&placement, &portable, &a, &b), PairClass::Portable);
        assert_eq!(pair_class(&placement, &portable, &a, &c), PairClass::Heavy);
        assert_eq!(pair_class(&placement, &HashSet::new(), &a, &b), PairClass::Heavy);
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
