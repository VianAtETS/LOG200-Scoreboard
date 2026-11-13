//! Paires tirées au début de la pause d'un slot et lancées à sa fin : la page les annonce, exactes,
//! dans `queue.json`. Une annonce tombe si une de ses équipes change de jar, quitte le roster ou
//! part en tournoi.

use std::collections::HashSet;
use std::fs;
use std::time::SystemTime;

use serde::Serialize;

use crate::roster::Team;

#[derive(Serialize)]
struct Announced {
    teams: [String; 2],
    /// Fin de la pause du slot, quand la paire sera lancée.
    at: String,
    #[serde(skip)]
    after: [String; 2],

    #[serde(skip)]
    pair: [Team; 2],
    #[serde(skip)]
    jars: [Option<SystemTime>; 2],
    #[serde(skip)]
    ready: bool,
}

#[derive(Default, Serialize)]
#[serde(transparent)]
pub struct Announcements(Vec<Announced>);

/// Date du jar actif : une soumission le remplace, donc la change.
fn jar_stamp(team: &Team) -> Option<SystemTime> {
    fs::metadata(team.build_file()?).and_then(|m| m.modified()).ok()
}

impl Announcements {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Équipes réservées par une annonce.
    pub fn teams(&self) -> impl Iterator<Item = &str> {
        self.0.iter().flat_map(|a| a.teams.iter().map(String::as_str))
    }

    /// `after` : la paire en pause dont le slot accueillera `pair` ; `at` : la fin de cette pause.
    pub fn add(&mut self, after: [String; 2], pair: [Team; 2], at: String) {
        self.0.push(Announced {
            teams: [pair[0].id.clone(), pair[1].id.clone()],
            at,
            after,
            jars: [jar_stamp(&pair[0]), jar_stamp(&pair[1])],
            pair,
            ready: false,
        });
    }

    /// La pause de `after` est finie : son annonce peut partir.
    pub fn rested(&mut self, after: &[String; 2]) {
        for announced in self.0.iter_mut().filter(|a| &a.after == after) {
            announced.ready = true;
        }
    }

    /// Retire les annonces dont une équipe a changé de jar, a quitté `roster`, joue ailleurs
    /// (`busy`, hors de la paire qu'elle suit tant que dure sa pause) ou est attendue en tournoi.
    /// `true` si une tombe.
    pub fn cancel_stale(&mut self, roster: &[Team], busy: &HashSet<String>, waiting: &HashSet<String>) -> bool {
        let before = self.0.len();
        self.0.retain(|a| {
            a.pair.iter().zip(&a.jars).all(|(team, jar)| {
                let id = &team.id;
                let present = roster.iter().any(|t| &t.id == id && jar_stamp(t) == *jar);
                let resting = !a.ready && a.after.contains(id);
                present && !waiting.contains(id) && (!busy.contains(id) || resting)
            })
        });
        self.0.len() != before
    }

    /// Annonces dont la pause est finie, retirées de la liste : `(paire, multithread)`.
    pub fn take_ready(&mut self) -> Vec<[Team; 2]> {
        let (ready, waiting) = std::mem::take(&mut self.0).into_iter().partition(|a| a.ready);
        self.0 = waiting;
        ready.into_iter().map(|a: Announced| a.pair).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandTemplate;
    use crate::submit::test_support::temp_dir;

    fn team(id: &str) -> Team {
        Team {
            id: id.into(),
            cmd: CommandTemplate::default(),
            build: None,
            anchor: false,
            multithread: false,
            remote: None,
        }
    }

    fn pair(a: &str, b: &str) -> [String; 2] {
        [a.into(), b.into()]
    }

    fn announced(after: [&str; 2], next: [&str; 2]) -> Announcements {
        let mut announcements = Announcements::default();
        announcements.add(
            pair(after[0], after[1]),
            next.map(team),
            "2026-11-05T12:01:00.000Z".into(),
        );
        announcements
    }

    fn roster(ids: &[&str]) -> Vec<Team> {
        ids.iter().map(|id| team(id)).collect()
    }

    #[test]
    fn publishes_the_pair_and_its_launch_time() {
        let json = serde_json::to_value(announced(["a", "b"], ["c", "d"])).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{ "teams": ["c", "d"], "at": "2026-11-05T12:01:00.000Z" }])
        );
    }

    #[test]
    fn leaves_only_once_its_slot_has_rested() {
        let mut announcements = announced(["a", "b"], ["a", "c"]);
        assert!(announcements.take_ready().is_empty());
        announcements.rested(&pair("x", "y"));
        assert!(announcements.take_ready().is_empty());
        announcements.rested(&pair("a", "b"));
        let ready = announcements.take_ready();
        assert_eq!(
            ready
                .iter()
                .map(|p| [p[0].id.as_str(), p[1].id.as_str()])
                .collect::<Vec<_>>(),
            [["a", "c"]]
        );
        assert_eq!(announcements.len(), 0);
    }

    #[test]
    fn a_resting_team_may_be_announced_again() {
        let mut announcements = announced(["a", "b"], ["a", "c"]);
        let busy = HashSet::from(["a".to_string(), "b".to_string()]);
        assert!(!announcements.cancel_stale(&roster(&["a", "b", "c"]), &busy, &HashSet::new()));
        assert_eq!(announcements.teams().collect::<Vec<_>>(), ["a", "c"]);
    }

    #[test]
    fn a_resting_team_taken_once_the_pause_is_over_cancels_it() {
        let mut announcements = announced(["a", "b"], ["a", "c"]);
        announcements.rested(&pair("a", "b"));
        let tournament = HashSet::from(["a".to_string()]);
        assert!(announcements.cancel_stale(&roster(&["a", "b", "c"]), &tournament, &HashSet::new()));
    }

    #[test]
    fn falls_when_a_team_plays_elsewhere_waits_for_a_tournament_or_leaves() {
        let all = roster(&["a", "b", "c", "d"]);
        let none = HashSet::new();
        let tournament = HashSet::from(["c".to_string()]);
        assert!(announced(["a", "b"], ["c", "d"]).cancel_stale(&all, &tournament, &none));
        assert!(announced(["a", "b"], ["c", "d"]).cancel_stale(&all, &none, &tournament));
        assert!(announced(["a", "b"], ["c", "d"]).cancel_stale(&roster(&["a", "b", "d"]), &none, &none));
        assert!(!announced(["a", "b"], ["c", "d"]).cancel_stale(&all, &none, &none));
    }

    #[test]
    fn falls_when_a_team_submits_a_new_jar() {
        let dir = temp_dir("announce-jar");
        let jar = dir.join("current.jar");
        fs::write(&jar, "v1").unwrap();
        let with_jar = |id: &str| Team {
            build: Some(jar.clone()),
            ..team(id)
        };
        let mut announcements = Announcements::default();
        announcements.add(pair("a", "b"), [with_jar("c"), team("d")], String::new());
        let all = [team("a"), team("b"), with_jar("c"), team("d")];
        assert!(!announcements.cancel_stale(&all, &HashSet::new(), &HashSet::new()));
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        fs::File::options()
            .write(true)
            .open(&jar)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(announcements.cancel_stale(&all, &HashSet::new(), &HashSet::new()));
        let _ = fs::remove_dir_all(&dir);
    }
}
