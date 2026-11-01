//! Répliques du CPU du serveur officiel (`cpu`), moteurs des niveaux 4 et 5 (`engine`) et
//! stratégies des bots.

pub mod cpu;
pub mod engine;
pub mod strategy;

#[cfg(test)]
mod golden;
