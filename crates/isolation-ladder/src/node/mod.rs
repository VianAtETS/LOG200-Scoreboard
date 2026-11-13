//! Nœuds de calcul : d'autres machines du LAN qui se connectent au ladder et y jouent les paires
//! portables, avec le même exécuteur que le serveur principal (`exec.rs`).
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "branché par l'agent et le registre des nœuds")
)]

pub mod protocol;

pub mod jars;
