//! Threads de partie : chacun joue sa paire ou son match de tournoi et rend compte à la boucle.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime};

use isolation_core::board::{Color, Move};
use isolation_core::record::{GameRecord, Player, Reason, TournamentGame, rfc3339_at};

use crate::arena::{Arena, Fixture, GameError};
use crate::config::Config;
use crate::hub::Hub;
use crate::placement::{Lease, PairClass, SlotKind};

use crate::remote::Registry;
use crate::roster::Team;
use crate::tournament::Pending;

/// Pause après une partie annulée : une config cassée ne doit pas faire tourner le ladder à vide.
const RETRY_DELAY: Duration = Duration::from_secs(5);
/// Avant de rejouer une partie de tournoi annulée sur un `no_show` : le plus souvent, une machine
/// surchargée ou trop chaude, qu'on laisse souffler.
const NO_SHOW_RETRY_DELAY: Duration = Duration::from_secs(30);

/// Compte rendu d'un thread de paire, ou réveil à l'arrivée d'une équipe distante.
#[expect(clippy::large_enum_variant, reason = "six messages par paire")]
pub enum Report {
    Wake,
    Started {
        red: String,
        black: String,
        game: Option<TournamentGame>,
    },
    /// La partie a obtenu un slot et son id.
    Assigned {
        red: String,
        black: String,
        id: String,
    },
    Played {
        red: String,
        black: String,
        outcome: Result<GameRecord, GameError>,
        game: Option<TournamentGame>,
    },
    /// Ce build a dépassé son temps sur un nœud : il n'y retourne plus.
    NodeTimeout {
        team: String,
        build: Option<String>,
    },
    /// Les deux parties de la paire sont jouées : son slot entre en pause jusqu'à `at`.
    Resting(Rest),
    /// Le bail est rendu à la boucle, qui le libère.
    Done {
        pair: [String; 2],
        lease: Lease,
    },
}

pub struct Rest {
    pub pair: [String; 2],
    /// Genre du slot à pourvoir : la paire annoncée doit y tenir.
    pub kind: SlotKind,
    pub at: String,
}

/// Registre des équipes distantes, si `[remote]` est configuré : il réveille la boucle à chaque arrivée.
pub fn remote_registry(cfg: &Config, reports: &Sender<Report>) -> Option<Arc<Registry>> {
    cfg.remote.as_ref()?;
    let wake = reports.clone();
    let interval = cfg.submit.as_ref().map_or(0, |submit| submit.min_interval_s);
    Some(Registry::new(Duration::from_secs(interval), move || {
        let _ = wake.send(Report::Wake);
    }))
}

/// `live.json` apprend l'id de chaque partie lancée.
pub fn live_hub(reports: &Sender<Report>) -> Arc<Hub> {
    let assigned = reports.clone();
    Hub::new(move |id, red, black| {
        let _ = assigned.send(Report::Assigned {
            red: red.into(),
            black: black.into(),
            id: id.into(),
        });
    })
}

/// Joue la paire dans les deux couleurs, avec la même ouverture, sur le slot de `lease`, gardé
/// jusqu'à la fin de la pause. Pendant l'arrêt, la seconde couleur n'est pas jouée.
pub fn spawn_pair(arena: Arc<Arena>, lease: Lease, a: Team, b: Team, opening: Vec<Move>, reports: Sender<Report>) {
    thread::spawn(move || {
        let mut slot = Some(&lease);
        for (red, black) in [(&a, &b), (&b, &a)] {
            let (retry, timed_out) = play_and_report(&arena, slot, &reports, red, black, &opening, None);
            if timed_out {
                slot = None;
            }
            if arena.draining() {
                break;
            }
            if let Some(delay) = retry {
                thread::sleep(delay);
            }
        }
        if !arena.draining() {
            let pause = Duration::from_secs(arena.config().pause_s);
            let _ = reports.send(Report::Resting(Rest {
                pair: [a.id.clone(), b.id.clone()],
                kind: lease.kind(),
                at: rfc3339_at(SystemTime::now() + pause),
            }));
            thread::sleep(pause);
        }
        report_done(&reports, a, b, lease);
    });
}

pub fn spawn_game(arena: Arc<Arena>, lease: Lease, p: Pending, reports: Sender<Report>) {
    thread::spawn(move || {
        let fixture = Fixture {
            game: p.game,
            time_ms: p.time_ms,
            forfeit_counts: p.forfeit_counts,
        };
        let (retry, _) = play_and_report(
            &arena,
            Some(&lease),
            &reports,
            &p.red,
            &p.black,
            &p.opening,
            Some(&fixture),
        );
        if let Some(delay) = retry {
            thread::sleep(delay);
        }
        report_done(&reports, p.red, p.black, lease);
    });
}

/// Une partie perdue avec son nœud n'a rien joué de compté : on la rejoue une fois, ailleurs.
fn replay<T>(
    first: Result<T, GameError>,
    again: impl FnOnce() -> Option<Result<T, GameError>>,
) -> Result<T, GameError> {
    match first {
        Err(GameError::NodeLost(_)) => again().unwrap_or(first),
        other => other,
    }
}

/// Un autre slot pour une paire portable dont le nœud est parti ; on attend une fois s'il n'y en a pas.
fn elsewhere(arena: &Arena) -> Option<Lease> {
    let placement = arena.placement();
    placement.try_lease(PairClass::Portable).or_else(|| {
        thread::sleep(RETRY_DELAY);
        placement.try_lease(PairClass::Portable)
    })
}

/// Le joueur qui a dépassé son temps sur un nœud : la partie ne compte pas.
fn node_timeout(record: &GameRecord) -> Option<&Player> {
    record.node.as_ref()?;
    let loser = match record.result.winner.other() {
        Color::Red => &record.red,
        Color::Black => &record.black,
    };
    (record.result.reason == Reason::Timeout).then_some(loser)
}

/// Joue une partie, sur `lease` ou, sans bail, sur un slot du serveur principal. Rend l'attente
/// avant la suivante si elle a échoué, et vrai si un joueur a dépassé son temps sur un nœud.
fn play_and_report(
    arena: &Arena,
    lease: Option<&Lease>,
    reports: &Sender<Report>,
    red: &Team,
    black: &Team,
    opening: &[Move],
    fixture: Option<&Fixture>,
) -> (Option<Duration>, bool) {
    let game = fixture.map(|f| f.game.clone());
    let _ = reports.send(Report::Started {
        red: red.id.clone(),
        black: black.id.clone(),
        game: game.clone(),
    });
    let live = !(red.is_bot() && black.is_bot());
    let first = arena.play(lease, red, black, opening, fixture, live);
    let outcome = replay(first, || {
        eprintln!(
            "   {} contre {} : nœud perdu, partie rejouée ailleurs",
            red.id, black.id
        );
        let other = elsewhere(arena)?;
        Some(arena.play(Some(&other), red, black, opening, fixture, live))
    });
    let late = outcome.as_ref().ok().and_then(node_timeout).cloned();
    let outcome = match &late {
        Some(player) => {
            eprintln!(
                "   {} : temps dépassé sur un nœud, partie rejouée au serveur principal",
                player.team
            );
            let _ = reports.send(Report::NodeTimeout {
                team: player.team.clone(),
                build: player.build.clone(),
            });
            arena.play(None, red, black, opening, fixture, live)
        }
        None => outcome,
    };
    let retry = match &outcome {
        Ok(_) => None,
        Err(GameError::NoShow(_)) => Some(NO_SHOW_RETRY_DELAY),
        Err(_) => Some(RETRY_DELAY),
    };
    let _ = reports.send(Report::Played {
        red: red.id.clone(),
        black: black.id.clone(),
        outcome,
        game,
    });
    (retry, late.is_some())
}

fn report_done(reports: &Sender<Report>, a: Team, b: Team, lease: Lease) {
    let _ = reports.send(Report::Done {
        pair: [a.id, b.id],
        lease,
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_game_lost_with_its_node_is_replayed_once_elsewhere() {
        let lost = || Err::<u32, _>(GameError::NodeLost("node-a".into()));
        let calls = Cell::new(0);
        let again = |result: Result<u32, GameError>| {
            calls.set(calls.get() + 1);
            Some(result)
        };
        assert_eq!(replay(lost(), || again(Ok(7))).unwrap(), 7);
        assert_eq!(calls.get(), 1);
        let twice = replay(lost(), || again(lost()));
        assert!(matches!(twice, Err(GameError::NodeLost(_))), "pas de troisième essai");
        assert_eq!(calls.get(), 2);
        assert!(
            matches!(replay(lost(), || None), Err(GameError::NodeLost(_))),
            "aucun slot libre"
        );
        let timeout = Err::<u32, _>(GameError::Timeout(Duration::from_secs(1)));
        assert!(matches!(replay(timeout, || again(Ok(1))), Err(GameError::Timeout(_))));
        assert_eq!(calls.get(), 2, "seule une perte de nœud est rejouée");
    }

    #[test]
    fn only_a_timeout_on_a_node_is_held_against_the_late_player() {
        let mut g = crate::db::tests::game("g", "a", "b", Color::Red);
        g.result.reason = Reason::Timeout;
        assert!(node_timeout(&g).is_none(), "au serveur principal, la partie compte");
        g.node = Some("node-a".into());
        assert_eq!(node_timeout(&g).map(|p| p.team.as_str()), Some("b"));
        g.result.reason = Reason::NoMoves;
        assert!(node_timeout(&g).is_none());
    }
}
