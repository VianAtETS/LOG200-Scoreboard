//! Registre des équipes inscrites. Le token d'une équipe n'est stocké nulle part en clair : seul
//! son sha256 l'est.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rand::RngExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use isolation::digest::{hex, sha256_hex};

pub const DEFAULT_TEAMS: &str = "jars/teams.toml";

/// Sert de nom de dossier et de conteneur : `a-z`, `0-9`, `-` et `_`, 32 caractères au plus,
/// commençant par une lettre ou un chiffre.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeamId(String);

#[derive(Debug, Error)]
#[error("id invalide : {0:?} (a-z, 0-9, - et _, 32 caractères au plus)")]
pub struct InvalidTeamId(String);

impl TryFrom<String> for TeamId {
    type Error = InvalidTeamId;

    fn try_from(id: String) -> Result<Self, InvalidTeamId> {
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
        let valid = (1..=32).contains(&id.len())
            && id.starts_with(allowed)
            && id.chars().all(|c| allowed(c) || c == '-' || c == '_');
        if valid { Ok(Self(id)) } else { Err(InvalidTeamId(id)) }
    }
}

impl FromStr for TeamId {
    type Err = InvalidTeamId;

    fn from_str(id: &str) -> Result<Self, InvalidTeamId> {
        id.to_string().try_into()
    }
}

impl From<TeamId> for String {
    fn from(id: TeamId) -> Self {
        id.0
    }
}

impl TeamId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Registered {
    pub id: TeamId,
    pub token_sha256: String,
}

#[derive(Serialize, Deserialize, Default)]
struct TeamsFile {
    #[serde(default, rename = "team")]
    teams: Vec<Registered>,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("{path} : {error}", path = .path.display())]
    Read { path: PathBuf, error: io::Error },
    #[error("{path} : {error}", path = .path.display())]
    Parse { path: PathBuf, error: toml::de::Error },
    #[error("{0} est déjà inscrite")]
    AlreadyRegistered(TeamId),
    #[error("écriture du registre : {0}")]
    Write(#[from] io::Error),
    #[error("écriture du registre : {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Un registre absent est vide ; un registre illisible est une erreur, pour ne jamais l'écraser.
pub fn load_teams(path: &Path) -> Result<Vec<Registered>, RegistryError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RegistryError::Read {
                path: path.to_path_buf(),
                error,
            });
        }
    };
    toml::from_str::<TeamsFile>(&text)
        .map(|file| file.teams)
        .map_err(|error| RegistryError::Parse {
            path: path.to_path_buf(),
            error,
        })
}

/// Inscrit une équipe et renvoie son token.
pub fn add_team(path: &Path, id: TeamId) -> Result<String, RegistryError> {
    let mut file = TeamsFile {
        teams: load_teams(path)?,
    };
    if file.teams.iter().any(|t| t.id == id) {
        return Err(RegistryError::AlreadyRegistered(id));
    }
    let token = hex(&rand::rng().random::<[u8; 24]>());
    file.teams.push(Registered {
        id,
        token_sha256: sha256_hex(token.as_bytes()),
    });
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, toml::to_string(&file)?)?;
    fs::rename(&tmp, path)?;
    Ok(token)
}

/// Équipe dont le token a ce sha256. Tout le registre est parcouru et chaque comparaison prend le
/// même temps : le temps de réponse ne doit rien apprendre sur les tokens.
pub fn find_team<'a>(teams: &'a [Registered], token: &str) -> Option<&'a Registered> {
    let hash = sha256_hex(token.as_bytes());
    teams.iter().fold(None, |found, team| {
        if same(&team.token_sha256, &hash) {
            Some(team)
        } else {
            found
        }
    })
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::submit::test_support::temp_dir;

    #[test]
    fn ids_are_safe_for_paths_and_containers() {
        for ok in ["eq07", "equipe_3", "a-b"] {
            assert!(ok.parse::<TeamId>().is_ok(), "{ok}");
        }
        for bad in ["", "../x", "Eq07", "-eq", "a b", "eq/7", &"x".repeat(33)] {
            assert!(bad.parse::<TeamId>().is_err(), "{bad}");
        }
    }

    #[test]
    fn add_team_stores_only_the_hash_and_finds_the_team_by_token() {
        let path = temp_dir("teams").join("teams.toml");
        let id = |s: &str| s.parse::<TeamId>().unwrap();
        let token = add_team(&path, id("eq07")).unwrap();
        let other = add_team(&path, id("eq08")).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains(&token));
        let teams = load_teams(&path).unwrap();
        assert_eq!(find_team(&teams, &token).unwrap().id, id("eq07"));
        assert_eq!(find_team(&teams, &other).unwrap().id, id("eq08"));
        assert!(find_team(&teams, "faux").is_none());
        assert!(matches!(
            add_team(&path, id("eq07")),
            Err(RegistryError::AlreadyRegistered(_))
        ));
    }

    #[test]
    fn corrupt_registry_is_never_overwritten() {
        let path = temp_dir("corrupt").join("teams.toml");
        fs::write(&path, "[[team]]\nid = \"eq07\"\n").unwrap();
        assert!(matches!(load_teams(&path), Err(RegistryError::Parse { .. })));
        assert!(add_team(&path, "eq08".parse().unwrap()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "[[team]]\nid = \"eq07\"\n");
        assert!(load_teams(&path.with_file_name("absent.toml")).unwrap().is_empty());
    }
}
