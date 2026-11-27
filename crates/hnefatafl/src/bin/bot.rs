//! Bot de Hnefatafl : joue au hasard parmi les coups légaux (adversaire des smoke tests).

use std::process::ExitCode;

fn main() -> ExitCode {
    game_kit::bot::main(
        &hnefatafl::Hnefatafl,
        "hnefatafl-bot",
        "Joue une partie de Hnefatafl 13×13 au hasard, contre un arbitre",
    )
}
