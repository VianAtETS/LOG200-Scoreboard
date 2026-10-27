//! `ladder.toml`. Une clé inconnue est une erreur : en TOML, une clé écrite sous la mauvaise
//! `[table]` y serait sinon ignorée sans bruit.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use isolation::board::Color;

use crate::submit::TeamId;

/// Une partie détruit au plus une case par coup.
const MAX_PLIES: u64 = 144;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `{port}`, `{time_ms}`, `{opening}` et `{game}` sont remplacés.
    pub referee: CommandTemplate,
    #[serde(default = "default_port")]
    pub port: u16,
    pub time_ms: u64,
    pub session: String,
    #[serde(default = "default_out")]
    pub out: PathBuf,
    /// Demi-coups aléatoires imposés en début de partie (pair). 0 si un client ignore le plateau initial.
    #[serde(default = "default_opening_plies")]
    pub opening_plies: usize,
    /// Débuts de lignes attendus sur le stderr de l'arbitre. Vides : attente fixe de `startup_ms`.
    #[serde(default = "default_ready_marker")]
    pub ready_marker: String,
    #[serde(default = "default_red_connected_marker")]
    pub red_connected_marker: String,
    #[serde(default = "default_startup_ms")]
    pub startup_ms: u64,
    /// Lancée après chaque partie, réussie ou non (ex. `docker rm -f` des conteneurs), et au
    /// démarrage avec `{game}` vide pour ramasser les restes d'un arrêt brutal.
    #[serde(default)]
    pub cleanup: CommandTemplate,
    /// Par défaut : 144 coups au temps max, plus 2 minutes.
    pub game_timeout_s: Option<u64>,
    /// Pause entre deux paires ; 0 les enchaîne.
    #[serde(default)]
    pub pause_s: u64,
    /// Exporté en variables d'environnement `SANDBOX_<CLÉ>` pour l'arbitre, les joueurs et `cleanup`.
    #[serde(default)]
    pub sandbox: BTreeMap<String, toml::Value>,
    /// Une partie simultanée par slot, chacun avec ses remplacements de `[sandbox]`
    #[serde(default, rename = "slot")]
    pub slots: Vec<BTreeMap<String, toml::Value>>,
    /// Commande des équipes inscrites ; `{jar}` devient le chemin de leur jar actif.
    #[serde(default = "default_team_cmd")]
    pub team_cmd: CommandTemplate,
    /// Soumission des jars par HTTP. Absente : seules les équipes de `[[team]]` jouent.
    pub submit: Option<SubmitConfig>,
    #[serde(default, rename = "team")]
    pub teams: Vec<Team>,
}

fn default_port() -> u16 {
    8888
}
fn default_out() -> PathBuf {
    "data/games.jsonl".into()
}
fn default_opening_plies() -> usize {
    2
}
fn default_ready_marker() -> String {
    "LISTENING".into()
}
fn default_red_connected_marker() -> String {
    "CONNECTED red".into()
}
fn default_startup_ms() -> u64 {
    1000
}
fn default_team_cmd() -> CommandTemplate {
    CommandTemplate::new(["sandbox/run-player.sh", "{game}", "{side}", "{port}", "{jar}"])
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("opening_plies doit être pair, pas {0}")]
    OddOpening(usize),
}

impl FromStr for Config {
    type Err = ConfigError;

    fn from_str(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text)?;
        if !config.opening_plies.is_multiple_of(2) {
            return Err(ConfigError::OddOpening(config.opening_plies));
        }
        Ok(config)
    }
}

impl Config {
    /// Dossier de `out`, servi à la page : `summary.json` et `teams/` y sont écrits.
    pub fn data_dir(&self) -> &Path {
        self.out.parent().unwrap_or(Path::new("."))
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir().join("logs")
    }

    /// Sortie d'erreur de `team` pendant la partie `game`.
    pub fn player_log(&self, game: &str, team: &str) -> PathBuf {
        self.logs_dir().join(format!("{game}-{team}.log"))
    }

    /// Parties simultanées : au moins une.
    pub fn slots(&self) -> usize {
        self.slots.len().max(1)
    }

    pub fn game_timeout(&self) -> Duration {
        Duration::from_secs(self.game_timeout_s.unwrap_or(MAX_PLIES * self.time_ms / 1000 + 120))
    }

    pub fn team_with_jar(&self, id: &TeamId, jar: &Path) -> Team {
        Team {
            id: id.to_string(),
            cmd: self.team_cmd.with_jar(&jar.to_string_lossy()),
            build: Some(jar.to_path_buf()),
            anchor: false,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: String,
    /// `{port}`, `{game}` et `{side}` (red ou black) sont remplacés.
    pub cmd: CommandTemplate,
    /// Fichier dont le sha256 identifie la soumission. Par défaut, le premier `.jar` de `cmd`.
    pub build: Option<PathBuf>,
    #[serde(default)]
    pub anchor: bool,
}

impl Team {
    pub fn build_file(&self) -> Option<PathBuf> {
        self.build.clone().or_else(|| self.cmd.jar().map(PathBuf::from))
    }
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct SubmitConfig {
    pub listen: String,
    /// Équipes inscrites et sha256 de leur token (`isolation-ladder add-team`).
    pub teams: PathBuf,
    /// Un dossier par équipe : `current.jar`, `history/`. Doit rester privé.
    pub jars: PathBuf,
    pub max_jar_mb: u64,
    /// Délai minimal entre deux soumissions d'une même équipe.
    pub min_interval_s: u64,
    /// Adversaire du smoke test, même gabarit que `cmd` des équipes.
    pub opponent: CommandTemplate,
}

impl Default for SubmitConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8090".into(),
            teams: crate::submit::DEFAULT_TEAMS.into(),
            jars: "jars".into(),
            max_jar_mb: 20,
            min_interval_s: 300,
            opponent: CommandTemplate::new(["sandbox/run-player.sh", "{game}", "{side}", "{port}", "--bot", "random"]),
        }
    }
}

impl SubmitConfig {
    pub fn team_dir(&self, id: &TeamId) -> PathBuf {
        self.jars.join(id.as_str())
    }

    pub fn current_jar(&self, id: &TeamId) -> PathBuf {
        self.team_dir(id).join("current.jar")
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

    fn jar(&self) -> Option<&str> {
        self.0
            .iter()
            .map(String::as_str)
            .find(|a| Path::new(a).extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
    }

    fn with_jar(&self, jar: &str) -> Self {
        Self(self.0.iter().map(|a| a.replace("{jar}", jar)).collect())
    }

    #[cfg(test)]
    pub fn args(&self) -> &[String] {
        &self.0
    }
}

/// Valeurs d'une partie, substituées dans les gabarits.
pub struct Vars<'a> {
    pub cfg: &'a Config,
    pub slot: usize,
    pub opening: String,
    pub game: &'a str,
}

impl Vars<'_> {
    /// `side` remplace `{side}` ; vide pour l'arbitre et `cleanup`.
    pub fn command(&self, template: &CommandTemplate, side: Option<Color>) -> io::Result<Command> {
        let port = self.cfg.port.to_string();
        let time_ms = self.cfg.time_ms.to_string();
        let side = side.map_or("", Color::name);
        let mut args = template.0.iter().map(|arg| {
            arg.replace("{port}", &port)
                .replace("{time_ms}", &time_ms)
                .replace("{opening}", &self.opening)
                .replace("{game}", self.game)
                .replace("{side}", side)
        });
        let program = args.next().ok_or_else(|| io::Error::other("commande vide"))?;
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null());
        let overrides = self.cfg.slots.get(self.slot).into_iter().flatten();
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
    use super::*;

    const MINIMAL: &str = "referee = [\"r\", \"{port}\", \"{opening}\"]\ntime_ms = 1000\nsession = \"s\"\n";

    #[test]
    fn example_config_parses() {
        let cfg: Config = include_str!("../../../ladder.example.toml").parse().unwrap();
        let submit = cfg.submit.as_ref().unwrap();
        assert_eq!(submit.max_jar_mb, 20);
        assert_eq!(cfg.team_cmd.args().last().map(String::as_str), Some("{jar}"));
    }

    #[test]
    fn misplaced_key_is_rejected() {
        let text = format!("{MINIMAL}[submit]\nteam_cmd = [\"x\"]\n");
        assert!(matches!(text.parse::<Config>(), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn odd_opening_is_rejected() {
        let text = format!("opening_plies = 3\n{MINIMAL}");
        assert!(matches!(text.parse::<Config>(), Err(ConfigError::OddOpening(3))));
    }

    #[test]
    fn defaults_and_substitution() {
        let cfg: Config = MINIMAL.parse().unwrap();
        assert_eq!(cfg.game_timeout(), Duration::from_secs(264));
        let id: TeamId = "eq07".parse().unwrap();
        let team = cfg.team_with_jar(&id, Path::new("jars/eq07/current.jar"));
        assert_eq!(team.build_file(), Some(PathBuf::from("jars/eq07/current.jar")));
        let vars = Vars {
            cfg: &cfg,
            slot: 0,
            opening: "G1-G4-F9".into(),
            game: "g1",
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
    fn slot_overrides_sandbox_but_keeps_port() {
        let text = format!(
            "{MINIMAL}[sandbox]\ncpuset_red = \"2\"\nmemory = \"512m\"\n\
             [[slot]]\ncpuset_red = \"2\"\n[[slot]]\ncpuset_red = \"4\"\n"
        );
        let cfg: Config = text.parse().unwrap();
        assert_eq!(cfg.slots(), 2);
        let env = |slot| {
            let vars = Vars {
                cfg: &cfg,
                slot,
                opening: String::new(),
                game: "g1",
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
