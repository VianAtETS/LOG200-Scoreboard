//! Arbitre d'une partie d'Isolation : l'arbitre générique de `game_kit` avec les règles
//! d'Isolation. Le contrat avec le ladder est dans docs/formats.md.

use std::process::ExitCode;

fn main() -> ExitCode {
    game_kit::referee::main(
        &isolation::Isolation,
        "isolation-referee",
        "Arbitre une partie d'Isolation entre deux clients TCP",
    )
}
