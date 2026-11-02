use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::routing::{get, put};
use http_body_util::LengthLimitError;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::task::spawn_blocking;

use crate::arena::Arena;
use crate::submit::{Submissions, SubmitError};

/// Serves the API while it lives.
pub struct Api {
    _runtime: Runtime,
}

pub fn serve(arena: &Arc<Arena>) -> anyhow::Result<Option<Api>> {
    let Some(submit) = arena.config().submit.clone() else {
        return Ok(None);
    };
    let runtime = Runtime::new()?;
    let listener = runtime
        .block_on(TcpListener::bind(&submit.listen))
        .with_context(|| format!("écoute des soumissions sur {}", submit.listen))?;
    eprintln!("soumissions : PUT http://{}/api/submit", listener.local_addr()?);
    let router = router(Arc::new(Submissions::new(Arc::clone(arena), submit)));
    runtime.spawn(async move { axum::serve(listener, router).await });
    Ok(Some(Api { _runtime: runtime }))
}

fn router(submissions: Arc<Submissions>) -> Router {
    Router::new()
        .route("/api/health", get(async || "ok\n").fallback(not_found))
        .route(
            "/api/version",
            get(async || concat!(env!("CARGO_PKG_VERSION"), "\n")).fallback(not_found),
        )
        .route("/api/submit", put(submit).post(submit).fallback(not_found))
        .fallback(not_found)
        .layer(DefaultBodyLimit::disable())
        .with_state(submissions)
}

async fn not_found() -> SubmitError {
    SubmitError::NotFound
}

async fn submit(
    State(submissions): State<Arc<Submissions>>,
    headers: HeaderMap,
    body: Body,
) -> Result<String, SubmitError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let team = blocking({
        let submissions = Arc::clone(&submissions);
        move || submissions.authenticate(authorization.as_deref())
    })
    .await?;
    let jar = to_bytes(body, submissions.max_body())
        .await
        .map_err(|error| body_error(error, submissions.max_jar_mb()))?;
    blocking(move || submissions.submit(&team, &jar))
        .await
        .map(|accepted| format!("{accepted}\n"))
}

fn body_error(error: axum::Error, max_mb: u64) -> SubmitError {
    let error = error.into_inner();
    if error.is::<LengthLimitError>() {
        SubmitError::TooLarge { max_mb }
    } else {
        SubmitError::Read(std::io::Error::other(error))
    }
}

/// The registry is a file and the smoke test plays two games: neither may block the runtime.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    spawn_blocking(work)
        .await
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[tokio::test]
    async fn a_body_over_the_limit_is_too_large() {
        let limit = 8;
        let error = to_bytes(Body::from(vec![0; limit + 1]), limit).await.unwrap_err();
        assert_eq!(body_error(error, 1).status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
