//! `ladder.toml`. Une clé inconnue est une erreur : en TOML, une clé écrite sous la mauvaise
//! `[table]` y serait sinon ignorée sans bruit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::command::CommandTemplate;
use crate::team_id::TeamId;
use crate::tournament::calendar::Schedule;

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
    /// Ancien fichier des parties : son dossier reçoit les fichiers de la page, et il est importé une fois dans `db`.
    #[serde(default = "default_out")]
    pub out: PathBuf,
    /// Base SQLite, source de vérité. Hors du dossier servi : elle contient les soumissions.
    #[serde(default = "default_db")]
    pub db: PathBuf,
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
    pub slots: Vec<Slot>,
    /// Commande des équipes inscrites ; `{jar}` devient le chemin de leur jar actif.
    #[serde(default = "default_team_cmd")]
    pub team_cmd: CommandTemplate,
    /// Soumission des jars par HTTP. Absente : seules les équipes de `[[team]]` jouent.
    pub submit: Option<SubmitConfig>,
    /// Équipes qui jouent depuis leur machine. Demande `[submit]`, qui tient les tokens.
    pub remote: Option<RemoteConfig>,
    /// Direct des parties pour la page (`/api/live`). Absent : pas de direct.
    pub live: Option<LiveConfig>,
    pub tournament: Option<TournamentConfig>,
    /// Garde-fou thermique (`thermal.rs`). Absent : les parties partent quelle que soit la température.
    pub thermal: Option<ThermalConfig>,
    #[serde(default, rename = "team")]
    pub teams: Vec<TeamConfig>,
}

fn default_port() -> u16 {
    8888
}
fn default_out() -> PathBuf {
    "data/games.jsonl".into()
}
fn default_db() -> PathBuf {
    "state/ladder.db".into()
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
    #[error("[remote] demande [submit], qui tient les tokens des équipes")]
    RemoteWithoutSubmit,
    #[error("[remote] connect doit commencer par unix: ou tcp:, pas {0:?}")]
    BadConnect(String),
    #[error("[tournament] min_teams doit valoir au moins 2, pas {0}")]
    TournamentTooSmall(usize),
    #[error("[thermal] resume_c ({resume_c}) doit être sous pause_c ({pause_c})")]
    ThermalHysteresis { pause_c: f64, resume_c: f64 },
}

impl FromStr for Config {
    type Err = ConfigError;

    fn from_str(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text)?;
        if !config.opening_plies.is_multiple_of(2) {
            return Err(ConfigError::OddOpening(config.opening_plies));
        }
        if let Some(remote) = &config.remote {
            if config.submit.is_none() {
                return Err(ConfigError::RemoteWithoutSubmit);
            }
            if !["unix:", "tcp:"]
                .iter()
                .any(|scheme| remote.connect.starts_with(scheme))
            {
                return Err(ConfigError::BadConnect(remote.connect.clone()));
            }
        }
        if let Some(tournament) = &config.tournament
            && tournament.min_teams < 2
        {
            return Err(ConfigError::TournamentTooSmall(tournament.min_teams));
        }
        if let Some(t) = &config.thermal
            && t.resume_c >= t.pause_c
        {
            return Err(ConfigError::ThermalHysteresis {
                pause_c: t.pause_c,
                resume_c: t.resume_c,
            });
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

    pub fn multithread_slot(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|s| s.multithread)
    }

    /// Slots de ce genre ; sans `[[slot]]`, le slot implicite est ordinaire.
    pub fn slots_of(&self, multithread: bool) -> usize {
        (0..self.slots())
            .filter(|&i| self.multithread_slot(i) == multithread)
            .count()
    }

    pub fn tournament_settings(&self) -> TournamentConfig {
        self.tournament.clone().unwrap_or_default()
    }

    pub fn game_timeout(&self, time_ms: u64) -> Duration {
        Duration::from_secs(self.game_timeout_s.unwrap_or(MAX_PLIES * time_ms / 1000 + 120))
    }
}

#[derive(Deserialize, Default)]
pub struct Slot {
    /// Réservé aux parties d'une équipe `multithread`, qui seules y jouent.
    #[serde(default)]
    pub multithread: bool,
    #[serde(flatten)]
    pub sandbox: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TeamConfig {
    pub id: String,
    /// `{port}`, `{game}` et `{side}` (red ou black) sont remplacés.
    pub cmd: CommandTemplate,
    /// Fichier dont le sha256 identifie la soumission. Par défaut, le premier `.jar` de `cmd`.
    pub build: Option<PathBuf>,
    #[serde(default)]
    pub anchor: bool,
    /// Ses parties se jouent dans les slots `multithread`, adversaire compris.
    #[serde(default)]
    pub multithread: bool,
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct RemoteConfig {
    pub listen: String,
    pub connect: String,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8091".into(),
            connect: "unix:/tmp/isolation-ladder/{game}/{side}/sock".into(),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct LiveConfig {
    pub listen: String,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8092".into(),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct TournamentConfig {
    pub weekly: Option<Schedule>,
    pub time_ms: u64,
    pub min_teams: usize,
    pub catch_up_h: u64,
    pub retries: u32,
}

impl Default for TournamentConfig {
    fn default() -> Self {
        Self {
            weekly: None,
            time_ms: 5000,
            min_teams: 4,
            catch_up_h: 2,
            retries: 3,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ThermalConfig {
    /// En °C. Au-dessus, aucune nouvelle partie.
    pub pause_c: f64,
    /// En °C, sous `pause_c`. En dessous, les parties reprennent.
    pub resume_c: f64,
    /// Fichier sysfs en millidegrés. Par défaut : le package CPU, trouvé sous `/sys/class`.
    pub sensor: Option<PathBuf>,
}

impl Default for ThermalConfig {
    fn default() -> Self {
        Self {
            pause_c: 90.0,
            resume_c: 80.0,
            sensor: None,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct SubmitConfig {
    pub listen: String,
    /// Équipes inscrites et sha256 de leur token (`isolation-ladder add-team`).
    pub teams: PathBuf,
    /// Un dossier par équipe : `current.jar` seulement. Doit rester privé.
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

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const MINIMAL: &str = "referee = [\"r\", \"{port}\", \"{opening}\"]\ntime_ms = 1000\nsession = \"s\"\n";

    #[test]
    fn example_config_parses() {
        let cfg: Config = include_str!("../../../ladder.example.toml").parse().unwrap();
        let submit = cfg.submit.as_ref().unwrap();
        assert_eq!(submit.max_jar_mb, 20);
        assert!(cfg.tournament.as_ref().unwrap().weekly.is_some());
        assert_eq!(cfg.team_cmd.args().last().map(String::as_str), Some("{jar}"));
    }

    #[test]
    fn tournament_defaults_to_five_seconds_without_a_schedule() {
        assert!(MINIMAL.parse::<Config>().unwrap().tournament.is_none());
        let cfg: Config = format!("{MINIMAL}[tournament]\n").parse().unwrap();
        let t = cfg.tournament.unwrap();
        assert_eq!(
            (t.weekly, t.time_ms, t.min_teams, t.catch_up_h, t.retries),
            (None, 5000, 4, 2, 3)
        );
    }

    #[test]
    fn tournament_schedule_and_size_are_validated() {
        let cfg: Config = format!("{MINIMAL}[tournament]\nweekly = \"wed 07:00, sun 19:00\"\n")
            .parse()
            .unwrap();
        assert_eq!(
            cfg.tournament
                .unwrap()
                .weekly
                .map(|s| s.0.iter().map(|w| w.weekday).collect()),
            Some(vec![jiff::civil::Weekday::Wednesday, jiff::civil::Weekday::Sunday])
        );
        let bad = format!("{MINIMAL}[tournament]\nweekly = \"dimanche midi\"\n");
        assert!(matches!(bad.parse::<Config>(), Err(ConfigError::Toml(_))));
        let tiny = format!("{MINIMAL}[tournament]\nmin_teams = 1\n");
        assert!(matches!(
            tiny.parse::<Config>(),
            Err(ConfigError::TournamentTooSmall(1))
        ));
    }

    #[test]
    fn misplaced_key_is_rejected() {
        let text = format!("{MINIMAL}[submit]\nteam_cmd = [\"x\"]\n");
        assert!(matches!(text.parse::<Config>(), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn remote_needs_submit_and_a_known_connect_scheme() {
        let without_submit = format!("{MINIMAL}[remote]\n");
        assert!(matches!(
            without_submit.parse::<Config>(),
            Err(ConfigError::RemoteWithoutSubmit)
        ));
        let bad = format!("{MINIMAL}[submit]\n[remote]\nconnect = \"/tmp/sock\"\n");
        assert!(matches!(bad.parse::<Config>(), Err(ConfigError::BadConnect(_))));
        let cfg: Config = format!("{MINIMAL}[submit]\n[remote]\n").parse().unwrap();
        assert!(cfg.remote.unwrap().connect.starts_with("unix:"));
    }

    #[test]
    fn thermal_defaults_and_hysteresis_are_checked() {
        assert!(MINIMAL.parse::<Config>().unwrap().thermal.is_none());
        let cfg: Config = format!("{MINIMAL}[thermal]\n").parse().unwrap();
        let t = cfg.thermal.unwrap();
        assert_eq!((t.pause_c, t.resume_c, t.sensor), (90.0, 80.0, None));
        let inverted = format!("{MINIMAL}[thermal]\npause_c = 80\nresume_c = 85\n");
        assert!(matches!(
            inverted.parse::<Config>(),
            Err(ConfigError::ThermalHysteresis { .. })
        ));
    }

    #[test]
    fn odd_opening_is_rejected() {
        let text = format!("opening_plies = 3\n{MINIMAL}");
        assert!(matches!(text.parse::<Config>(), Err(ConfigError::OddOpening(3))));
    }
}
