//! Arbitre d'une partie de Hnefatafl : l'arbitre générique de `game_kit`, avec les règles et le
//! protocole du serveur officiel de LOG320 (docs/rules-hnefatafl.md).

use std::process::ExitCode;

fn main() -> ExitCode {
    game_kit::referee::main(
        &hnefatafl::Hnefatafl,
        "hnefatafl-referee",
        "Arbitre une partie de Hnefatafl 13×13 entre deux clients TCP",
    )
}
