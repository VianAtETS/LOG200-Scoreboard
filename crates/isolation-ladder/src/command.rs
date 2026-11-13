use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

use isolation_core::board::Color;

use crate::config::{
    Config, Slot, default_port, default_ready_marker, default_red_connected_marker, default_startup_ms,
};

/// Ce qu'il faut pour lancer une partie sur une machine : l'arbitre, les joueurs, le bac à sable et
/// ses slots. Au serveur principal, tiré de `ladder.toml` ; sur un nœud, sa table `[exec]`.
#[derive(Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ExecConfig {
    pub referee: CommandTemplate,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_ready_marker")]
    pub ready_marker: String,
    #[serde(default = "default_red_connected_marker")]
    pub red_connected_marker: String,
    #[serde(default = "default_startup_ms")]
    pub startup_ms: u64,
    #[serde(default)]
    pub cleanup: CommandTemplate,
    #[serde(default)]
    pub sandbox: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "slot")]
    pub slots: Vec<Slot>,
    /// `[remote].connect` : où relier une équipe distante. Jamais sur un nœud.
    #[serde(skip)]
    pub remote_connect: Option<String>,
}

impl From<&Config> for ExecConfig {
    fn from(cfg: &Config) -> Self {
        Self {
            referee: cfg.referee.clone(),
            port: cfg.port,
            ready_marker: cfg.ready_marker.clone(),
            red_connected_marker: cfg.red_connected_marker.clone(),
            startup_ms: cfg.startup_ms,
            cleanup: cfg.cleanup.clone(),
            sandbox: cfg.sandbox.clone(),
            slots: cfg.slots.clone(),
            remote_connect: cfg.remote.as_ref().map(|r| r.connect.clone()),
        }
    }
}

/// Commande dont les arguments contiennent des `{variables}`.
#[derive(Deserialize, Clone, Default, Debug)]
#[serde(transparent)]
pub struct CommandTemplate(Vec<String>);

impl CommandTemplate {
    pub fn new<const N: usize>(args: [&str; N]) -> Self {
        Self(args.map(String::from).to_vec())
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn jar(&self) -> Option<&str> {
        self.0
            .iter()
            .map(String::as_str)
            .find(|a| Path::new(a).extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
    }

    pub fn with_jar(&self, jar: &str) -> Self {
        Self(self.0.iter().map(|a| a.replace("{jar}", jar)).collect())
    }

    pub fn then(&self, args: impl IntoIterator<Item = String>) -> Self {
        Self(self.0.iter().cloned().chain(args).collect())
    }

    /// Stratégie et options d'un bot de référence (`... --bot niveau-2 --margin-ms 100`).
    pub fn bot(&self) -> Option<(&str, &[String])> {
        let at = self.0.iter().position(|a| a == "--bot")?;
        let strategy = self.0.get(at + 1)?;
        Some((strategy, &self.0[at + 2..]))
    }

    #[cfg(test)]
    pub fn args(&self) -> &[String] {
        &self.0
    }
}

/// Valeurs d'une partie, substituées dans les gabarits.
pub struct Vars<'a> {
    pub cfg: &'a ExecConfig,

    pub slot: usize,
    pub opening: String,
    pub game: &'a str,
    pub time_ms: u64,
}

impl Vars<'_> {
    /// `side` remplace `{side}` ; vide pour l'arbitre et `cleanup`.
    pub fn substitute(&self, arg: &str, side: Option<Color>) -> String {
        arg.replace("{port}", &self.cfg.port.to_string())
            .replace("{time_ms}", &self.time_ms.to_string())
            .replace("{opening}", &self.opening)
            .replace("{game}", self.game)
            .replace("{side}", side.map_or("", Color::name))
    }

    pub fn command(&self, template: &CommandTemplate, side: Option<Color>) -> io::Result<Command> {
        let mut args = template.0.iter().map(|arg| self.substitute(arg, side));
        let program = args.next().ok_or_else(|| io::Error::other("commande vide"))?;
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null());
        let overrides = self.cfg.slots.get(self.slot).into_iter().flat_map(|s| &s.sandbox);
        for (key, value) in self.cfg.sandbox.iter().chain(overrides) {
            let value = match value {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            command.env(format!("SANDBOX_{}", key.to_uppercase()), value);
        }
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::config::tests::MINIMAL;
    use crate::roster::Team;
    use crate::team_id::TeamId;

    #[test]
    fn bot_commands_parse_into_a_strategy_and_args() {
        let cmd = CommandTemplate::new([
            "sandbox/run-player.sh",
            "{game}",
            "{side}",
            "{port}",
            "--bot",
            "niveau-2",
            "--margin-ms",
            "100",
        ]);
        assert_eq!(
            cmd.bot(),
            Some(("niveau-2", ["--margin-ms".to_string(), "100".to_string()].as_slice()))
        );
        assert_eq!(CommandTemplate::new(["run.sh", "{jar}"]).bot(), None);
        assert_eq!(CommandTemplate::new(["run.sh", "--bot"]).bot(), None);
    }

    #[test]
    fn the_exec_config_follows_the_ladder_config() {
        let text = format!("{MINIMAL}cleanup = [\"c\", \"{{game}}\"]\n[[slot]]\ncpus = \"2\"\n");
        let cfg: Config = text.parse().unwrap();
        assert_eq!(cfg.exec.port, 8888);
        assert_eq!(cfg.exec.slots.len(), 1);
        assert!(!cfg.exec.cleanup.is_empty());
        assert_eq!(cfg.exec.ready_marker, "LISTENING");
        let node: ExecConfig = toml::from_str("referee = [\"r\"]\n[[slot]]\ncpuset_red = \"4\"\n").unwrap();
        assert_eq!((node.port, node.slots.len()), (8888, 1));
        assert!(toml::from_str::<ExecConfig>("referee = [\"r\"]\nsession = \"s\"\n").is_err());
    }

    #[test]
    fn defaults_and_substitution() {
        let cfg: Config = MINIMAL.parse().unwrap();
        assert_eq!(cfg.game_timeout(cfg.time_ms), Duration::from_secs(264));
        let id: TeamId = "eq07".parse().unwrap();
        let team = Team::with_jar(&cfg, &id, Path::new("jars/eq07/current.jar"), false);
        assert_eq!(team.build_file(), Some(PathBuf::from("jars/eq07/current.jar")));
        let vars = Vars {
            cfg: &cfg.exec,
            slot: 0,
            opening: "G1-G4-F9".into(),
            game: "g1",
            time_ms: cfg.time_ms,
        };
        let command = vars.command(&cfg.referee, None).unwrap();
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["8888", "G1-G4-F9"]);
        let player = vars.command(&team.cmd, Some(Color::Black)).unwrap();
        assert!(player.get_args().any(|a| a == "black"));
        assert!(player.get_args().any(|a| a == "jars/eq07/current.jar"));
        assert_eq!(cfg.slots(), 1);
    }

    #[test]
    fn tournament_time_replaces_the_ladder_time() {
        let cfg: Config = "referee = [\"r\", \"{time_ms}\"]\ntime_ms = 500\nsession = \"s\"\n"
            .parse()
            .unwrap();
        let vars = Vars {
            cfg: &cfg.exec,
            slot: 0,
            opening: String::new(),
            game: "g1",
            time_ms: 5000,
        };
        let command = vars.command(&cfg.referee, None).unwrap();
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["5000"]);
        assert_eq!(cfg.game_timeout(5000), Duration::from_mins(14));
    }

    #[test]
    fn slot_overrides_sandbox_but_keeps_port() {
        let text = format!(
            "{MINIMAL}[sandbox]\ncpuset_red = \"2\"\nmemory = \"512m\"\n\
             [[slot]]\ncpuset_red = \"2\"\n[[slot]]\ncpuset_red = \"4\"\n"
        );
        let cfg: Config = text.parse().unwrap();
        assert_eq!(cfg.slots(), 2);
        let env = |slot| {
            let vars = Vars {
                cfg: &cfg.exec,
                slot,
                opening: String::new(),
                game: "g1",
                time_ms: cfg.time_ms,
            };
            let command = vars.command(&cfg.referee, None).unwrap();
            let port = command.get_args().next().unwrap().to_string_lossy().into_owned();
            let var = |key: &str| {
                command
                    .get_envs()
                    .find(|&(k, _)| k == key)
                    .and_then(|(_, v)| v)
                    .map(|v| v.to_string_lossy().into_owned())
            };
            (port, var("SANDBOX_CPUSET_RED"), var("SANDBOX_MEMORY"))
        };
        assert_eq!(env(0), ("8888".into(), Some("2".into()), Some("512m".into())));
        assert_eq!(env(1), ("8888".into(), Some("4".into()), Some("512m".into())));
    }
}
