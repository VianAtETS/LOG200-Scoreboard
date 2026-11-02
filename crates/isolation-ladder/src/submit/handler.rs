use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Cursor};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

use isolation_core::digest::short_hash;
use isolation_core::record::now_rfc3339;

use super::auth::{RegistryError, bearer, find_team, load_teams};
use super::manifest::{self, BadThreads};
use super::smoke::{Forfeit, Verdict, smoke_test};
use crate::arena::{Arena, GameError};
use crate::config::SubmitConfig;
use crate::store::append_jsonl;
use crate::sync::lock;
use crate::team_id::TeamId;

#[derive(Debug, PartialEq, Eq)]
pub enum Accepted {
    AlreadyActive(String),
    Installed { hash: String, multithread: bool },
}

impl fmt::Display for Accepted {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AlreadyActive(hash) => write!(f, "déjà actif : ce jar ({hash}) est celui qui joue"),
            Self::Installed { hash, multithread } => {
                write!(f, "accepté : version {hash}, en jeu dès la prochaine partie")?;
                if *multithread {
                    f.write_str(" (Ladder-Threads: 2 : 2 coeurs par couleur, adversaire compris)")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SubmitError {
    #[error("inconnu : seul PUT /api/submit existe")]
    NotFound,
    #[error("token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)")]
    Unauthorized,
    #[error("lecture du jar : {0}")]
    Read(io::Error),
    #[error("jar trop gros (max {max_mb} Mo)")]
    TooLarge { max_mb: u64 },
    #[error("le corps de la requête n'est pas un jar")]
    NotAJar,
    #[error(transparent)]
    BadThreads(#[from] BadThreads),
    #[error("trop tôt : nouvelle soumission possible dans {} s", .0.as_secs() + 1)]
    TooSoon(Duration),
    #[error("registre des équipes : {0}")]
    Registry(RegistryError),
    #[error("écriture du jar : {0}")]
    Stage(io::Error),
    #[error("refusé ({0}) : le jar actif reste en jeu{tail}", tail = .0.stderr_section())]
    Rejected(Forfeit),
    #[error("erreur du ladder pendant le smoke test, réessayer plus tard : {0}")]
    Ladder(GameError),
    #[error("installation du jar : {0}")]
    Install(io::Error),
    #[error("ladder en redémarrage : réessayer dans quelques minutes")]
    Draining,
}

impl SubmitError {
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Read(_) | Self::NotAJar | Self::BadThreads(_) => StatusCode::BAD_REQUEST,
            Self::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooSoon(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Registry(_) | Self::Stage(_) | Self::Ladder(_) | Self::Install(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Draining => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl IntoResponse for SubmitError {
    fn into_response(self) -> Response {
        (self.status(), format!("{self}\n")).into_response()
    }
}

/// Délai minimal entre deux soumissions d'une même équipe.
struct RateLimiter {
    interval: Duration,
    last: HashMap<TeamId, Instant>,
}

impl RateLimiter {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: HashMap::new(),
        }
    }

    /// Réserve le créneau de `team`, ou renvoie l'attente restante.
    fn acquire(&mut self, team: &TeamId, now: Instant) -> Result<(), Duration> {
        if let Some(&last) = self.last.get(team)
            && let Some(left) = self.interval.checked_sub(now.saturating_duration_since(last))
            && !left.is_zero()
        {
            return Err(left);
        }
        self.last.insert(team.clone(), now);
        Ok(())
    }

    /// Rend le créneau quand l'échec vient du ladder, pas de l'équipe.
    fn release(&mut self, team: &TeamId) {
        self.last.remove(team);
    }
}

pub struct Submissions {
    arena: Arc<Arena>,
    submit: SubmitConfig,
    limiter: Mutex<RateLimiter>,
}

impl Submissions {
    pub fn new(arena: Arc<Arena>, submit: SubmitConfig) -> Self {
        let limiter = Mutex::new(RateLimiter::new(Duration::from_secs(submit.min_interval_s)));
        Self { arena, submit, limiter }
    }

    /// One byte over the limit tells an oversized body apart from a full one.
    pub fn max_body(&self) -> usize {
        usize::try_from(max_bytes(self.submit.max_jar_mb) + 1).unwrap_or(usize::MAX)
    }

    pub const fn max_jar_mb(&self) -> u64 {
        self.submit.max_jar_mb
    }

    pub fn submit(&self, team: &TeamId, jar: &[u8]) -> Result<Accepted, SubmitError> {
        check_jar(jar, self.submit.max_jar_mb)?;
        let multithread = manifest::multithread(Cursor::new(jar))?;
        let hash = short_hash(jar);
        if fs::read(self.submit.current_jar(team)).is_ok_and(|active| short_hash(&active) == hash) {
            return Ok(Accepted::AlreadyActive(hash));
        }
        let _hold = self.arena.hold().ok_or(SubmitError::Draining)?;
        self.limiter()
            .acquire(team, Instant::now())
            .map_err(SubmitError::TooSoon)?;
        let outcome = self.test_and_install(team, &hash, jar, multithread);
        if matches!(outcome, Err(SubmitError::Ladder(_))) {
            self.limiter().release(team);
        }
        outcome
    }

    /// Tenu le temps d'un `acquire` ou d'un `release` : les smoke tests attendent leur tour dans `Arena`.
    fn limiter(&self) -> MutexGuard<'_, RateLimiter> {
        lock(&self.limiter)
    }

    pub fn authenticate(&self, authorization: Option<&str>) -> Result<TeamId, SubmitError> {
        let token = authorization.and_then(bearer).ok_or(SubmitError::Unauthorized)?;
        let teams = load_teams(&self.submit.teams).map_err(SubmitError::Registry)?;
        find_team(&teams, token)
            .map(|team| team.id.clone())
            .ok_or(SubmitError::Unauthorized)
    }

    fn test_and_install(
        &self,
        team: &TeamId,
        hash: &str,
        jar: &[u8],
        multithread: bool,
    ) -> Result<Accepted, SubmitError> {
        let dir = self.submit.team_dir(team);
        let staging = dir.join(format!("incoming-{hash}.jar"));
        fs::create_dir_all(&dir)
            .and_then(|()| fs::write(&staging, jar))
            .map_err(SubmitError::Stage)?;
        eprintln!("soumission {team} {hash} : smoke test...");
        let outcome = match smoke_test(&self.arena, &self.submit, team, &staging) {
            Verdict::Accepted => install(&dir, &staging)
                .map(|()| Accepted::Installed {
                    hash: hash.to_string(),
                    multithread,
                })
                .map_err(SubmitError::Install),
            Verdict::Rejected(forfeit) => Err(SubmitError::Rejected(forfeit)),
            Verdict::Error(error) => Err(SubmitError::Ladder(error)),
        };
        if matches!(outcome, Err(SubmitError::Rejected(_) | SubmitError::Ladder(_))) {
            let _ = fs::remove_file(&staging);
        }
        self.log(team, hash, &outcome);
        outcome
    }

    fn log(&self, team: &TeamId, hash: &str, outcome: &Result<Accepted, SubmitError>) {
        let (verdict, detail) = match outcome {
            Ok(_) => (LogVerdict::Accepted, String::new()),
            Err(SubmitError::Rejected(forfeit)) => (LogVerdict::Rejected, forfeit.to_string()),
            Err(SubmitError::Ladder(error)) => (LogVerdict::Error, error.to_string()),
            Err(error) => (LogVerdict::Error, error.to_string()),
        };
        eprintln!("soumission {team} {hash} : {} {detail}", <&str>::from(verdict));
        let line = LogLine {
            ts: now_rfc3339(),
            team,
            hash,
            verdict,
            detail: &detail,
        };
        let _ = append_jsonl(&self.submit.jars.join("submissions.jsonl"), &line);
    }
}

#[derive(Serialize, Clone, Copy)]
#[serde(into = "&str")]
enum LogVerdict {
    Accepted,
    Rejected,
    Error,
}

impl From<LogVerdict> for &'static str {
    fn from(verdict: LogVerdict) -> Self {
        match verdict {
            LogVerdict::Accepted => "accepted",
            LogVerdict::Rejected => "rejected",
            LogVerdict::Error => "error",
        }
    }
}

#[derive(Serialize)]
struct LogLine<'a> {
    ts: String,
    team: &'a TeamId,
    hash: &'a str,
    verdict: LogVerdict,
    detail: &'a str,
}

const fn max_bytes(max_mb: u64) -> u64 {
    max_mb * 1024 * 1024
}

fn check_jar(jar: &[u8], max_mb: u64) -> Result<(), SubmitError> {
    if u64::try_from(jar.len()).map_or(true, |len| len > max_bytes(max_mb)) {
        return Err(SubmitError::TooLarge { max_mb });
    }
    if !jar.starts_with(b"PK\x03\x04") {
        return Err(SubmitError::NotAJar);
    }
    Ok(())
}

/// Remplace atomiquement le jar actif ; l'ancien n'est pas gardé.
fn install(dir: &Path, staging: &Path) -> io::Result<()> {
    fs::rename(staging, dir.join("current.jar"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::submit::add_team;
    use crate::submit::test_support::temp_dir;

    const JAR: &[u8] = b"PK\x03\x04 un jar";

    /// Un ladder dont l'arbitre ne peut pas se lancer : toute partie échoue tout de suite.
    fn submissions(name: &str) -> (Submissions, String) {
        let dir = temp_dir(name);
        let toml = format!(
            "referee = [\"isolation-arbitre-absent\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\n\
             [submit]\nteams = {teams:?}\njars = {jars:?}\nmax_jar_mb = 1\n",
            out = dir.join("games.jsonl"),
            teams = dir.join("teams.toml"),
            jars = dir.join("jars"),
        );
        let cfg: crate::config::Config = toml.parse().unwrap();
        let submit = cfg.submit.clone().unwrap();
        let token = add_team(&submit.teams, "eq07".parse().unwrap()).unwrap();
        (
            Submissions::new(Arc::new(Arena::new(cfg, crate::hub::Hub::new(|_, _, _| {}))), submit),
            token,
        )
    }

    fn team() -> TeamId {
        "eq07".parse().unwrap()
    }

    #[test]
    fn only_a_registered_bearer_token_authenticates() {
        let (_, stranger) = submissions("auth-stranger");
        let (submissions, token) = submissions("auth");
        assert_eq!(
            submissions.authenticate(Some(&format!("Bearer {token}"))).unwrap(),
            team()
        );
        for header in [None, Some(token.as_str()), Some(&format!("Bearer {stranger}"))] {
            let error = submissions.authenticate(header).unwrap_err();
            assert_eq!(error.status(), StatusCode::UNAUTHORIZED, "{header:?}");
        }
    }

    #[test]
    fn a_body_that_is_not_a_jar_or_too_large_is_refused() {
        let (submissions, _) = submissions("checks");
        let not_a_jar = submissions.submit(&team(), &JAR[1..]).unwrap_err();
        assert_eq!(not_a_jar.status(), StatusCode::BAD_REQUEST);
        let huge = [JAR, &vec![0; submissions.max_body()]].concat();
        assert_eq!(
            submissions.submit(&team(), &huge).unwrap_err().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn unknown_thread_count_is_refused_before_the_smoke_test() {
        let (submissions, _) = submissions("threads");
        let jar = manifest::tests::jar_with_manifest("Main-Class: Client\nLadder-Threads: 8\n");
        let error = submissions.submit(&team(), &jar).unwrap_err();
        assert!(matches!(error, SubmitError::BadThreads(_)), "{error}");
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert!(!submissions.submit.jars.join("submissions.jsonl").exists());
    }

    #[test]
    fn active_jar_is_not_tested_again() {
        let (submissions, _) = submissions("active");
        let id: TeamId = "eq07".parse().unwrap();
        let current = submissions.submit.current_jar(&id);
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::write(&current, JAR).unwrap();
        let reply = submissions.submit(&team(), JAR).unwrap();
        assert_eq!(reply, Accepted::AlreadyActive(short_hash(JAR)));
    }

    #[test]
    fn ladder_failure_is_logged_and_does_not_count_against_the_team() {
        let (submissions, _) = submissions("ladder-error");
        for _ in 0..2 {
            let error = submissions.submit(&team(), JAR).unwrap_err();
            assert!(matches!(error, SubmitError::Ladder(GameError::Spawn { .. })), "{error}");
            assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }
        let id: TeamId = "eq07".parse().unwrap();
        let dir = submissions.submit.team_dir(&id);
        assert!(!dir.join(format!("incoming-{}.jar", short_hash(JAR))).exists());
        let log = fs::read_to_string(submissions.submit.jars.join("submissions.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 2);
        assert!(log.contains("\"verdict\":\"error\""));
    }

    #[test]
    fn a_draining_ladder_refuses_without_using_the_team_slot() {
        let (submissions, _) = submissions("draining");
        submissions.arena.drain();
        let error = submissions.submit(&team(), JAR).unwrap_err();
        assert!(matches!(error, SubmitError::Draining), "{error}");
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(submissions.arena.smoke_tests(), 0);
        assert!(!submissions.submit.jars.join("submissions.jsonl").exists());
        let team: TeamId = "eq07".parse().unwrap();
        assert_eq!(submissions.limiter().acquire(&team, Instant::now()), Ok(()));
    }

    #[test]
    fn rate_limiter_counts_from_the_last_submission() {
        let mut limiter = RateLimiter::new(Duration::from_secs(300));
        let team: TeamId = "eq07".parse().unwrap();
        let start = Instant::now();
        assert_eq!(limiter.acquire(&team, start), Ok(()));
        let wait = limiter.acquire(&team, start + Duration::from_secs(100)).unwrap_err();
        assert_eq!(wait, Duration::from_secs(200));
        assert_eq!(
            SubmitError::TooSoon(wait).to_string(),
            "trop tôt : nouvelle soumission possible dans 201 s"
        );
        assert_eq!(limiter.acquire(&team, start + Duration::from_secs(300)), Ok(()));
        limiter.release(&team);
        assert_eq!(limiter.acquire(&team, start + Duration::from_secs(301)), Ok(()));
    }

    #[test]
    fn install_replaces_current_without_keeping_old_versions() {
        let dir = temp_dir("install");
        fs::write(dir.join("current.jar"), b"old").unwrap();
        let staging = dir.join("incoming-abc.jar");
        fs::write(&staging, b"new").unwrap();
        install(&dir, &staging).unwrap();
        assert_eq!(fs::read(dir.join("current.jar")).unwrap(), b"new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    }
}
