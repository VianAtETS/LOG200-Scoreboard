//! Spec `OpenAPI` de l'API, servi sur `/api/openapi.json` et affiché par la page `api.html`. Les
//! statistiques sont du JSON fait en SQL : les types ci-dessous le décrivent, et les tests de
//! `db.rs` vérifient que chaque requête y correspond.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "API du ladder Isolation",
        description = "Parties, statistiques et soumission des jars du ladder LOG200. Les routes de \
            lecture sont publiques ; la soumission demande le token de l'équipe (bouton *Authorize*).\n\n\
            Deux WebSockets ne figurent pas ici : `/api/live` (parties en cours) et `/api/remote` \
            (jouer depuis sa machine), décrites dans \
            [formats.md](https://github.com/VianAtETS/LOG200-Scoreboard/blob/main/docs/formats.md)."
    ),
    tags(
        (name = "parties", description = "Parties complètes, coups compris."),
        (name = "statistiques", description = "Statistiques calculées dans la base, rafraîchies toutes les 15 s."),
        (name = "soumission", description = "Envoi d'un jar, avec le token de l'équipe."),
        (name = "profileur", description = "Profil des jars : choix et lecture avec le token de l'équipe, qui ne voit que les siens."),
        (name = "serveur", description = "État du ladder."),
    ),
    modifiers(&Token)
)]
pub struct ApiDoc;

struct Token;

impl Modify for Token {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.components.get_or_insert_default().add_security_scheme(
            "token",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

/// Un jar, envoyé tel quel.
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct Jar(#[allow(dead_code, reason = "schéma seulement : le handler lit le corps brut")] Vec<u8>);

/// Victoires et parties dans une couleur.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ColorRecord {
    pub games: u64,
    pub wins: u64,
}

/// Bilan d'une équipe.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TeamRecord {
    pub team: String,
    pub games: u64,
    pub wins: u64,
    pub red: ColorRecord,
    pub black: ColorRecord,
    /// Défaites par raison (`no_moves`, `timeout`, `disconnect`, `protocol`, `no_show`).
    pub losses: BTreeMap<String, u64>,
}

/// Une paire d'équipes qui se sont rencontrées, `a` avant `b` dans l'ordre alphabétique.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HeadToHead {
    pub a: String,
    pub b: String,
    pub games: u64,
    pub a_wins: u64,
    pub b_wins: u64,
}

/// Le profil du jar d'une équipe dans une partie, privé : son côté seulement.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GameProfile {
    pub game: String,
    /// `red` ou `black`.
    pub color: String,
    pub build: String,
    /// Segments de calcul vus par le profileur (deux tours rapides peuvent n'en faire qu'un).
    pub turns: u64,
    /// Coups joués par le jar.
    pub moves: u64,
    /// Temps pris par l'échantillonnage, compté dans la réflexion du jar.
    pub overhead_ms: u64,
    pub tree: crate::profile::Tree,
    /// Ses lignes `ARENA`, s'il en imprime.
    pub arena: Vec<game_kit::record::Measure>,
}

/// Une paire complète du ladder : deux parties, même ouverture, couleurs inversées.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DuelPair {
    /// Les ids des deux parties, dans l'ordre de jeu.
    pub games: [String; 2],
    /// L'équipe gagnante de chaque partie, dans le même ordre.
    pub winners: [String; 2],
    pub opening: Vec<String>,
}

/// Une ouverture imposée par le ladder et les victoires du rouge.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningRecord {
    pub opening: Vec<String>,
    pub games: u64,
    pub red_wins: u64,
}

/// Un premier coup client et les victoires de la couleur qui l'a joué.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveRecord {
    #[serde(rename = "move")]
    pub mv: String,
    pub games: u64,
    pub wins: u64,
}

/// Les 50 premiers coups clients les plus joués, par couleur.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FirstMoves {
    pub red: Vec<MoveRecord>,
    pub black: Vec<MoveRecord>,
}

/// Les 50 ouvertures les plus jouées et les premiers coups clients.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Openings {
    pub openings: Vec<OpeningRecord>,
    pub first_moves: FirstMoves,
}

/// Bilan d'un build (16 premiers caractères hexadécimaux du sha256 du jar).
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    pub build: String,
    pub games: u64,
    pub wins: u64,
    /// Horodatage de sa première partie.
    pub first: String,
    /// Horodatage de sa dernière partie.
    pub last: String,
}

/// Un rating et sa variation sur la partie.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingStep {
    pub rating: f64,
    pub delta: f64,
}

/// Rating de l'équipe après une partie classée.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingPoint {
    pub game: String,
    pub ts: String,
    pub session: String,
    /// Toutes sessions confondues.
    pub all: RatingStep,
    /// Dans la session de la partie.
    pub in_session: RatingStep,
}

/// Temps de réflexion d'un build.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveTimes {
    /// `null` pour les bots du ladder.
    pub build: Option<String>,
    /// Coups chronométrés.
    pub moves: u64,
    pub avg_ms: u64,
    pub max_ms: u64,
    /// Coups à 90 % ou plus du temps par coup, sans le premier de chaque partie (une seconde de
    /// grâce pour la JVM à froid).
    pub near_limit: u64,
}

/// Cartes de chaleur d'un build : par case (`F4`), combien de fois le pion de l'équipe y est arrivé
/// et combien de fois elle l'a détruite. Une case jamais touchée n'y figure pas.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Heatmap {
    /// `null` pour les bots du ladder.
    pub build: Option<String>,
    pub games: u64,
    pub moves: BTreeMap<String, u64>,
    pub blocks: BTreeMap<String, u64>,
}

/// Une soumission de jar et son verdict.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmissionRecord {
    pub ts: String,
    pub hash: String,
    /// `accepted`, `rejected` ou `error`.
    pub verdict: String,
    /// Raison d'un rejet ; vide sinon (une erreur du ladder peut citer des chemins du serveur).
    pub detail: String,
}

#[cfg(test)]
pub fn check(stat: crate::db::Stat, json: &str) {
    use crate::db::Stat;
    fn parse<T: serde::de::DeserializeOwned>(json: &str) {
        if let Err(e) = serde_json::from_str::<T>(json) {
            panic!("la requête ne correspond plus à son schéma OpenAPI : {e}\n{json}");
        }
    }
    match stat {
        Stat::Teams => parse::<Vec<TeamRecord>>(json),
        Stat::HeadToHead => parse::<Vec<HeadToHead>>(json),
        Stat::Openings => parse::<Openings>(json),
        Stat::Builds => parse::<Vec<BuildRecord>>(json),
        Stat::Ratings => parse::<Vec<RatingPoint>>(json),
        Stat::MoveTimes => parse::<Vec<MoveTimes>>(json),
        Stat::Heatmap => parse::<Vec<Heatmap>>(json),
        Stat::Submissions => parse::<Vec<SubmissionRecord>>(json),
    }
}
