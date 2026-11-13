//! Enchaîne les parties entre équipes, une paire par slot libre, et enregistre chaque partie en
//! base. Les équipes sont celles de `ladder.toml` plus les équipes inscrites qui ont un jar
//! accepté ou une session distante ouverte ; la liste est relue avant chaque paire.

// Démarrage et boucle
mod cli;
mod config;
mod ladder;
mod migrate;

// Matchmaking et parties
mod arena;
mod command;
mod exec;
mod node;
mod pairs;
mod placement;
mod play;
mod roster;
mod schedule;
mod thermal;
mod tournament;

// Équipes
mod remote;
mod submit;
mod team_id;

// Résultats et publication
mod api;
mod db;
mod hub;
mod live;
mod model;
mod openapi;
mod spectate;
mod standings;

// Utilitaires
mod store;
mod sync;

pub use cli::cli;
