//! Mesures des bots du ladder : une ligne `ARENA depth=6 nodes=123456` par coup sur leur sortie
//! d'erreur, au format de l'arène isola de Justin Pagès (gitlab.com/justin-pages/isola). Lues dans
//! leur journal à la fin de la partie et publiées avec elle : le code des bots est public.

use std::fs;
use std::path::Path;

use isolation_core::record::Measure;

/// La mesure d'une ligne `ARENA` ; les autres clés et les valeurs illisibles sont ignorées.
pub fn parse(line: &str) -> Option<Measure> {
    let mut measure = Measure::default();
    for (key, value) in line
        .strip_prefix("ARENA ")?
        .split_whitespace()
        .filter_map(|t| t.split_once('='))
    {
        match key {
            "depth" => measure.depth = value.parse().ok(),
            "nodes" => measure.nodes = value.parse().unwrap_or_default(),
            _ => {}
        }
    }
    Some(measure)
}

/// Les mesures d'un journal, si elles sont une par coup : sinon elles ne correspondraient pas aux
/// coups (journal coupé, bot interrompu), et rien n'est publié.
fn of_log(log: &str, moves: usize) -> Vec<Measure> {
    let measures: Vec<Measure> = log.lines().filter_map(parse).collect();
    if measures.len() == moves { measures } else { Vec::new() }
}

/// Les mesures du journal `path` d'un bot qui a joué `moves` coups ; vide sans journal lisible.
pub fn read(path: &Path, moves: usize) -> Vec<Measure> {
    fs::read(path).map_or_else(|_| Vec::new(), |bytes| of_log(&String::from_utf8_lossy(&bytes), moves))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_measure_per_move_from_the_arena_lines_only() {
        let log = "nouvelle partie : rouge, 1000 ms\n\
                   ARENA depth=6 nodes=123456\n\
                   <- D10-C10-I7\n\
                   ARENA nodes=0\n\
                   ARENA depth=x nodes=7 maMesure=3.2\n\
                   fin : victoire\n";
        let measured = |depth, nodes| Measure { depth, nodes };
        assert_eq!(
            of_log(log, 3),
            [measured(Some(6), 123_456), measured(None, 0), measured(None, 7)]
        );
        assert!(of_log(log, 4).is_empty(), "une mesure manquante : rien de publié");
        assert!(of_log("fin\n", 0).is_empty());
        assert!(read(Path::new("/absent/journal.log"), 3).is_empty());
    }
}
