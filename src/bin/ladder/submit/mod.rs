//! Soumission des jars : `PUT /api/submit` avec `Authorization: Bearer <token>` et le jar en corps.
//! Un jar qui passe le smoke test remplace atomiquement le jar actif de l'équipe. Chaque requête a
//! son thread : `/api/health` et les refus rapides n'attendent pas les smoke tests, qui se jouent un
//! à la fois, après la partie en cours du ladder.

mod auth;
mod handler;
mod manifest;
mod smoke;

use std::sync::Arc;
use std::thread;

use anyhow::anyhow;

pub use auth::{DEFAULT_TEAMS, RegistryError, TeamId, add_team, find_team, load_teams};
use handler::{Request, Submissions};
pub use smoke::SMOKE_OPPONENT;

use crate::arena::Arena;
use crate::config::{Config, Team};

/// Équipes inscrites dont un jar a été accepté.
pub fn active_teams(cfg: &Config) -> Result<Vec<Team>, RegistryError> {
    let Some(submit) = &cfg.submit else {
        return Ok(Vec::new());
    };
    Ok(auth::load_teams(&submit.teams)?
        .into_iter()
        .filter_map(|registered| {
            let jar = submit.current_jar(&registered.id);
            jar.exists()
                .then(|| cfg.team_with_jar(&registered.id, &jar, manifest::multithread_file(&jar)))
        })
        .collect())
}

/// Sert les soumissions dans un thread, si `[submit]` est configuré.
pub fn serve(arena: Arc<Arena>) -> anyhow::Result<()> {
    let Some(submit) = arena.config().submit.clone() else {
        return Ok(());
    };
    let server = tiny_http::Server::http(&submit.listen)
        .map_err(|e| anyhow!("écoute des soumissions sur {} : {e}", submit.listen))?;
    eprintln!("soumissions : PUT http://{}/api/submit", submit.listen);
    let submissions = Arc::new(Submissions::new(arena, submit));
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let submissions = Arc::clone(&submissions);
            thread::spawn(move || respond(&submissions, request));
        }
    });
    Ok(())
}

fn respond(submissions: &Submissions, mut request: tiny_http::Request) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let authorization = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.to_string());
    let reply = submissions.handle(Request {
        method: &method,
        url: &url,
        authorization: authorization.as_deref(),
        body: request.as_reader(),
    });
    let (status, body) = match reply {
        Ok(accepted) => (200, accepted.to_string()),
        Err(error) => (error.status(), error.to_string()),
    };
    let _ = request.respond(tiny_http::Response::from_string(format!("{body}\n")).with_status_code(status));
}

#[cfg(test)]
pub mod test_support {
    use std::path::PathBuf;

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("isolation-submit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
