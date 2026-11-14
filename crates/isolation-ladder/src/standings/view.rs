use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;

use isolation_core::board::Color;
use isolation_core::record::{Reason, TournamentGame};

use crate::db::Rating;

#[derive(Serialize, Clone)]
pub(super) struct Side {
    pub(super) team: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) build: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) remote: bool,
    /// Rating après la partie et variation, toutes sessions puis dans la session.
    pub(super) all: [f64; 2],
    pub(super) session: [f64; 2],
    /// Les mêmes sans les parties distantes, s'ils diffèrent. Absent d'une partie distante.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) verified: Option<Verified>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub(super) struct Verified {
    pub(super) all: [f64; 2],
    pub(super) session: [f64; 2],
}

/// Une partie telle que la page l'affiche, sans les coups (le replay passe par `/api/games/{id}`).
#[derive(Serialize, Clone)]
pub(super) struct Row {
    pub(super) v: u32,
    pub(super) id: String,
    pub(super) ts: String,
    pub(super) session: String,
    pub(super) time_ms: u64,
    pub(super) winner: Color,
    pub(super) reason: Reason,
    pub(super) moves: usize,
    pub(super) duration_ms: u64,
    pub(super) red: Side,
    pub(super) black: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tournament: Option<TournamentGame>,
    /// Jouée sur un nœud : ses temps par coup ne valent pas ceux du serveur principal. Le nom du
    /// nœud reste privé.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) node: bool,
}

/// Ligne de `timeline.jsonl` : le strict nécessaire pour rejouer les ratings de toutes les équipes.
#[derive(Serialize)]
pub(super) struct Tick<'a> {
    pub(super) v: u32,
    pub(super) id: &'a str,
    pub(super) ts: &'a str,
    pub(super) session: &'a str,
    pub(super) red: &'a Side,
    pub(super) black: &'a Side,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) tournament: bool,
}

impl Row {
    pub(super) fn tick(&self) -> Tick<'_> {
        Tick {
            v: self.v,
            id: &self.id,
            ts: &self.ts,
            session: &self.session,
            red: &self.red,
            black: &self.black,
            tournament: self.tournament.is_some(),
        }
    }

    pub(super) fn ratings(&self) -> [Rating; 2] {
        [&self.red, &self.black].map(|side| Rating {
            game: self.id.clone(),
            team: side.team.clone(),
            all: side.all,
            session: side.session,
        })
    }
}

#[derive(Serialize)]
pub(super) struct PaceView {
    pub(super) mean: u64,
    pub(super) p50: u64,
    pub(super) p95: u64,
    pub(super) p99: u64,
    pub(super) max: u64,
    pub(super) count: u64,
    pub(super) limit: u64,
}

#[derive(Serialize)]
pub(super) struct RatingRow<'a> {
    pub(super) team: &'a str,
    pub(super) rating: f64,
    pub(super) peak: f64,
    pub(super) games: u32,
    pub(super) wins: u32,
    pub(super) losses: u32,
    pub(super) build_games: u32,
    pub(super) build_wins: u32,
    pub(super) anchor: bool,
    pub(super) provisional: bool,
    pub(super) remote: bool,
    pub(super) build: Option<&'a str>,
    pub(super) last: &'a str,
    pub(super) pace: Option<PaceView>,
    /// Temps par coup des autres builds de l'équipe, par hash ; seulement avec `--archives`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) archived_paces: BTreeMap<&'a str, PaceView>,
}

#[derive(Serialize)]
pub(super) struct ScopeView<'a> {
    pub(super) games: u64,
    pub(super) time_ms: &'a BTreeSet<u64>,
    pub(super) ratings: Vec<RatingRow<'a>>,
    pub(super) recent: &'a VecDeque<Row>,
}

#[derive(Serialize)]
pub(super) struct Summary<'a> {
    pub(super) v: u32,
    /// Filtres de versions archivées activés au démarrage ; absent par défaut.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) archives: bool,
    pub(super) sessions: Vec<&'a str>,
    pub(super) builds: &'a BTreeMap<String, Vec<String>>,
    pub(super) scopes: BTreeMap<&'a str, ScopeView<'a>>,
    /// Mêmes clés que `scopes`, sans les parties distantes ; absent tant qu'il n'y en a pas eu.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) verified: Option<BTreeMap<&'a str, ScopeView<'a>>>,
}
