//! `node.toml`, la configuration d'un nœud. Une clé inconnue est une erreur, comme pour le ladder.

use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

use crate::command::{CommandTemplate, ExecConfig};
use crate::config::{ThermalConfig, default_team_cmd};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    /// Nom du nœud, celui de `isolation-ladder add-node` au ladder.
    pub id: String,
    /// `ws://<ladder>:<port>/api/node`, sur le LAN.
    pub ladder: String,
    /// Fichier du token donné par `add-node`.
    pub token_file: PathBuf,
    #[serde(default = "default_jars")]
    pub jars: PathBuf,
    #[serde(default = "default_cache_mb")]
    pub cache_mb: u64,
    #[serde(default = "default_max_jar_mb")]
    pub max_jar_mb: u64,
    /// Sorties d'erreur des joueurs, le temps de les renvoyer au ladder.
    #[serde(default = "default_logs")]
    pub logs: PathBuf,
    /// Commande d'un bot de référence, suivie de sa stratégie et de ses options.
    pub bot_cmd: CommandTemplate,
    /// Les seuls bots que ce nœud accepte de lancer.
    pub bots: Vec<String>,
    /// Commande d'un jar d'équipe ; `{jar}` devient son chemin dans le cache.
    #[serde(default = "default_team_cmd")]
    pub team_cmd: CommandTemplate,
    pub thermal: Option<ThermalConfig>,
    /// L'arbitre, le bac à sable et les slots de cette machine.
    pub exec: ExecConfig,
}

fn default_jars() -> PathBuf {
    "/var/cache/isolation/jars".into()
}
fn default_cache_mb() -> u64 {
    512
}
fn default_max_jar_mb() -> u64 {
    20
}
fn default_logs() -> PathBuf {
    "/tmp/isolation-node/logs".into()
}

#[derive(Debug, Error)]
pub enum NodeConfigError {
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("id {0:?} : lettres, chiffres et tirets seulement")]
    BadId(String),
    #[error("ladder doit commencer par ws:// ou wss://, pas {0:?}")]
    BadLadder(String),
    #[error("bots est vide : la sonde de démarrage a besoin d'au moins un bot")]
    NoBots,
}

impl NodeConfig {
    /// Slots de la machine : au moins un.
    pub fn slots(&self) -> usize {
        self.exec.slots.len().max(1)
    }
}

impl FromStr for NodeConfig {
    type Err = NodeConfigError;

    fn from_str(text: &str) -> Result<Self, NodeConfigError> {
        let cfg: Self = toml::from_str(text)?;
        let id_ok = !cfg.id.is_empty() && cfg.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if !id_ok {
            return Err(NodeConfigError::BadId(cfg.id));
        }
        if !cfg.ladder.starts_with("ws://") && !cfg.ladder.starts_with("wss://") {
            return Err(NodeConfigError::BadLadder(cfg.ladder));
        }
        if cfg.bots.is_empty() {
            return Err(NodeConfigError::NoBots);
        }
        Ok(cfg)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const NODE: &str = "id = \"node-a\"\nladder = \"ws://127.0.0.1:8093/api/node\"\ntoken_file = \"token\"\n\
                            bot_cmd = [\"bot\", \"{side}\"]\nbots = [\"aleatoire\", \"niveau-1\"]\n\
                            [exec]\nreferee = [\"r\"]\n[[exec.slot]]\ncpuset_red = \"4\"\n[[exec.slot]]\ncpuset_red = \"6\"\n";

    #[test]
    fn a_node_config_parses_and_checks_what_it_cannot_do_without() {
        let cfg: NodeConfig = NODE.parse().unwrap();
        assert_eq!((cfg.slots(), cfg.cache_mb, cfg.max_jar_mb), (2, 512, 20));
        assert!(NODE.replace("\"node-a\"", "\"../x\"").parse::<NodeConfig>().is_err());
        assert!(NODE.replace("ws://", "http://").parse::<NodeConfig>().is_err());
        let no_bots = NODE.replace("[\"aleatoire\", \"niveau-1\"]", "[]");
        assert!(matches!(no_bots.parse::<NodeConfig>(), Err(NodeConfigError::NoBots)));
        assert!(format!("session = \"s\"\n{NODE}").parse::<NodeConfig>().is_err());
    }
}
