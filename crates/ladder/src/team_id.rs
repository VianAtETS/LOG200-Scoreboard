use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
