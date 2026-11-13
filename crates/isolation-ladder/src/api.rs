use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, State};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, LOCATION};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http_body_util::LengthLimitError;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::task::spawn_blocking;
use utoipa::{IntoParams, OpenApi};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouter};
use utoipa_axum::routes;

use isolation_core::record::GameRecord;
use isolation_core::remote::PATH as REMOTE_PATH;

use crate::arena::Arena;
use crate::db::{Db, GameFilter, Stat};
use crate::hub::Hub;
use crate::openapi::{
    ApiDoc, BuildRecord, HeadToHead, Jar, MoveTimes, Openings, RatingPoint, SubmissionRecord, TeamRecord,
};
use crate::remote::{self, Registry, Remote};
use crate::spectate;
use crate::submit::{Accepted, Submissions, SubmitError};
use crate::team_id::TeamId;

/// Serves the API while it lives.
pub struct Api {
    _runtime: Runtime,
}

/// One router on every configured address: the existing reverse proxies keep their routes.
pub fn serve(arena: &Arc<Arena>, hub: &Arc<Hub>, registry: Option<&Arc<Registry>>) -> anyhow::Result<Option<Api>> {
    let cfg = arena.config();
    let mut documented = OpenApiRouter::with_openapi(ApiDoc::openapi());
    let mut announced: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    if let Some(submit) = &cfg.submit {
        documented = documented.merge(submit_routes(Arc::new(Submissions::new(
            Arc::clone(arena),
            submit.clone(),
        ))));
        announced
            .entry(&submit.listen)
            .or_default()
            .push("soumissions : PUT http://{}/api/submit");
    }
    documented = documented.merge(read_routes(cfg.db.clone()));
    let (mut router, spec) = documented.split_for_parts();
    let spec = Bytes::from(spec.to_json()?);
    router = router.route(
        "/api/openapi.json",
        get(async move || {
            (
                [
                    (CONTENT_TYPE, "application/json"),
                    (CACHE_CONTROL, "public, max-age=300"),
                ],
                spec.clone(),
            )
        }),
    );
    if let Some(live) = &cfg.live {
        router = router.merge(
            Router::new()
                .route(spectate::PATH, get(spectate::watch))
                .with_state(Arc::clone(hub)),
        );
        announced
            .entry(&live.listen)
            .or_default()
            .push("direct : ws://{}/api/live");
    }
    if let (Some(remote), Some(submit), Some(registry)) = (&cfg.remote, &cfg.submit, registry) {
        let state = Remote::new(Arc::clone(registry), submit.teams.clone(), cfg.time_ms);
        router = router.merge(Router::new().route(REMOTE_PATH, get(remote::upgrade)).with_state(state));
        announced
            .entry(&remote.listen)
            .or_default()
            .push("équipes distantes : ws://{}/api/remote");
    }
    if let (Some(nodes), Some(hub)) = (&cfg.nodes, arena.nodes()) {
        router = router.merge(
            Router::new()
                .route(crate::node::protocol::PATH, get(crate::node::hub::upgrade))
                .with_state(Arc::clone(hub)),
        );
        announced
            .entry(&nodes.listen)
            .or_default()
            .push("nœuds : ws://{}/api/node (LAN seulement)");
    }
    if announced.is_empty() {
        return Ok(None);
    }

    let router = router.fallback(not_found).layer(DefaultBodyLimit::disable());
    let runtime = Runtime::new()?;
    for (address, services) in announced {
        let listener = runtime
            .block_on(TcpListener::bind(address))
            .with_context(|| format!("écoute de l'API sur {address}"))?;
        let local = listener.local_addr()?.to_string();
        for service in services {
            eprintln!("{}", service.replace("{}", &local));
        }
        runtime.spawn(
            axum::serve(
                listener,
                router.clone().into_make_service_with_connect_info::<SocketAddr>(),
            )
            .into_future(),
        );
    }
    Ok(Some(Api { _runtime: runtime }))
}

/// Another method on these paths answers like an unknown route.
fn or_404<S: Clone + Send + Sync + 'static>((schemas, paths, method): UtoipaMethodRouter<S>) -> UtoipaMethodRouter<S> {
    (schemas, paths, method.fallback(not_found))
}

fn submit_routes(submissions: Arc<Submissions>) -> OpenApiRouter {
    OpenApiRouter::new()
        .routes(or_404(routes!(health)))
        .routes(or_404(routes!(version)))
        .routes(or_404(routes!(submit)))
        .routes(or_404(routes!(verdict)))
        .with_state(SubmitState {
            submissions,
            jobs: Arc::new(Jobs::new()),
        })
}

/// Lectures publiques de la base, chacune sur sa propre connexion en lecture seule.
fn read_routes(db: PathBuf) -> OpenApiRouter {
    let recent = Recent {
        db: db.clone(),
        cache: Mutex::new(None),
    };
    OpenApiRouter::new()
        .routes(routes!(game))
        .routes(routes!(stats_teams))
        .routes(routes!(stats_head_to_head))
        .routes(routes!(stats_openings))
        .routes(routes!(stats_builds))
        .routes(routes!(stats_ratings))
        .routes(routes!(stats_move_times))
        .routes(routes!(stats_submissions))
        .with_state(Arc::new(db))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(recent_games))
                .with_state(Arc::new(recent)),
        )
}

/// Répond `ok` tant que le ladder tourne, pour la surveillance.
#[utoipa::path(get, path = "/api/health", tag = "serveur", responses(
    (status = 200, body = String, content_type = "text/plain", example = "ok\n"),
))]
async fn health() -> &'static str {
    "ok\n"
}

/// La version du ladder.
#[utoipa::path(get, path = "/api/version", tag = "serveur", responses(
    (status = 200, body = String, content_type = "text/plain", example = "0.29.4\n"),
))]
async fn version() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), "\n")
}

const RECENT_GAMES: usize = 5000;
const RECENT_TTL: Duration = Duration::from_secs(60);

/// Les dernières parties, relues au plus une fois par `RECENT_TTL` quel que soit le nombre d'appels.
struct Recent {
    db: PathBuf,
    cache: Mutex<Option<(Instant, Bytes)>>,
}

impl Recent {
    // Le verrou reste pris pendant la lecture : les appels simultanés attendent au lieu de relire la base.
    fn get(&self) -> rusqlite::Result<Bytes> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, body)) = &*cache
            && at.elapsed() < RECENT_TTL
        {
            return Ok(body.clone());
        }
        match Db::open_read(&self.db).and_then(|db| db.games_jsonl(&GameFilter::default(), RECENT_GAMES)) {
            Ok(text) => {
                let body = Bytes::from(text);
                *cache = Some((Instant::now(), body.clone()));
                Ok(body)
            }
            Err(e) => match &*cache {
                Some((_, stale)) => {
                    eprintln!("lecture de la base : {e} ; dernières parties servies depuis le cache");
                    Ok(stale.clone())
                }
                None => Err(e),
            },
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct Page {
    /// Nombre de parties, 5000 au plus (et par défaut).
    limit: Option<usize>,
}

/// Les parties, une par ligne, de la plus ancienne à la plus récente : les 5000 dernières, ou
/// celles qui passent les filtres. Sans paramètre, une copie relue au plus une fois par minute.
#[utoipa::path(get, path = "/api/games", tag = "parties", params(GameFilter, Page), responses(
    (status = 200, body = GameRecord, content_type = "application/x-ndjson",
     description = "Une partie par ligne (JSONL), vide si aucune ne correspond."),
    (status = 400, description = "Paramètre illisible."),
    (status = 500, description = "Base indisponible."),
))]
async fn recent_games(
    State(recent): State<Arc<Recent>>,
    Query(filter): Query<GameFilter>,
    Query(page): Query<Page>,
) -> Response {
    let (cache, result) = if filter == GameFilter::default() && page.limit.is_none() {
        ("public, max-age=60", blocking(move || recent.get()).await)
    } else {
        let limit = page.limit.unwrap_or(RECENT_GAMES).min(RECENT_GAMES);
        let read = move || Db::open_read(&recent.db)?.games_jsonl(&filter, limit).map(Bytes::from);
        ("public, max-age=15", blocking(read).await)
    };
    match result {
        Ok(body) => ([(CONTENT_TYPE, "application/x-ndjson"), (CACHE_CONTROL, cache)], body).into_response(),
        Err(e) => unavailable(&e),
    }
}

fn json(cache: &'static str, body: String) -> Response {
    ([(CONTENT_TYPE, "application/json"), (CACHE_CONTROL, cache)], body).into_response()
}

fn unavailable(error: &rusqlite::Error) -> Response {
    eprintln!("lecture de la base : {error}");
    (StatusCode::INTERNAL_SERVER_ERROR, "base indisponible\n").into_response()
}

/// Une partie, pour le replay. Une partie finie ne change plus : un CDN peut la garder indéfiniment.
#[utoipa::path(get, path = "/api/games/{id}", tag = "parties",
    params(("id" = String, Path, description = "Id de la partie", example = "20261024T142103.512Z-eq07-eq12")),
    responses(
        (status = 200, body = GameRecord),
        (status = 404, description = "Partie inconnue ou encore en cours."),
        (status = 500, description = "Base indisponible."),
    ),
)]
async fn game(State(db): State<Arc<PathBuf>>, UrlPath(id): UrlPath<String>) -> Response {
    match blocking(move || Db::open_read(&db)?.game_json(&id)).await {
        Ok(Some(body)) => json("public, max-age=31536000, immutable", body),
        Ok(None) => (StatusCode::NOT_FOUND, "partie inconnue\n").into_response(),
        Err(e) => unavailable(&e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct Scope {
    /// Une session (`automne-2026`) ou un tournoi (`tournoi-2026-11-08`). Absente : toutes les
    /// sessions, sans les tournois, comme le classement.
    session: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct TeamScope {
    /// L'équipe (`eq100`).
    team: String,
    /// Une session (`automne-2026`) ou un tournoi (`tournoi-2026-11-08`). Absente : toutes les
    /// sessions, sans les tournois, comme le classement.
    session: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct Team {
    /// L'équipe (`eq100`).
    team: String,
}

async fn stat(db: Arc<PathBuf>, stat: Stat, session: Option<String>, team: Option<String>) -> Response {
    match blocking(move || Db::open_read(&db)?.stats(stat, session.as_deref(), team.as_deref())).await {
        // Le rythme de rafraîchissement de la page, comme /data.
        Ok(body) => json("public, max-age=15", body),
        Err(e) => unavailable(&e),
    }
}

/// Bilan par équipe, classé par victoires puis par nombre de parties.
#[utoipa::path(get, path = "/api/stats/teams", tag = "statistiques", params(Scope), responses(
    (status = 200, body = Vec<TeamRecord>), (status = 500, description = "Base indisponible."),
))]
async fn stats_teams(State(db): State<Arc<PathBuf>>, Query(q): Query<Scope>) -> Response {
    stat(db, Stat::Teams, q.session, None).await
}

/// Face-à-face de chaque paire d'équipes qui se sont rencontrées.
#[utoipa::path(get, path = "/api/stats/head-to-head", tag = "statistiques", params(Scope), responses(
    (status = 200, body = Vec<HeadToHead>), (status = 500, description = "Base indisponible."),
))]
async fn stats_head_to_head(State(db): State<Arc<PathBuf>>, Query(q): Query<Scope>) -> Response {
    stat(db, Stat::HeadToHead, q.session, None).await
}

/// Ouvertures imposées et premiers coups clients les plus joués, avec leurs victoires.
#[utoipa::path(get, path = "/api/stats/openings", tag = "statistiques", params(Scope), responses(
    (status = 200, body = Openings), (status = 500, description = "Base indisponible."),
))]
async fn stats_openings(State(db): State<Arc<PathBuf>>, Query(q): Query<Scope>) -> Response {
    stat(db, Stat::Openings, q.session, None).await
}

/// Bilan de chaque build de l'équipe, dans l'ordre où ils ont joué pour la première fois.
#[utoipa::path(get, path = "/api/stats/builds", tag = "statistiques", params(TeamScope), responses(
    (status = 200, body = Vec<BuildRecord>), (status = 400, description = "`team` manquant."),
    (status = 500, description = "Base indisponible."),
))]
async fn stats_builds(State(db): State<Arc<PathBuf>>, Query(q): Query<TeamScope>) -> Response {
    stat(db, Stat::Builds, q.session, Some(q.team)).await
}

/// Rating de l'équipe après chacune de ses parties classées : sa courbe Elo.
#[utoipa::path(get, path = "/api/stats/ratings", tag = "statistiques", params(TeamScope), responses(
    (status = 200, body = Vec<RatingPoint>), (status = 400, description = "`team` manquant."),
    (status = 500, description = "Base indisponible."),
))]
async fn stats_ratings(State(db): State<Arc<PathBuf>>, Query(q): Query<TeamScope>) -> Response {
    stat(db, Stat::Ratings, q.session, Some(q.team)).await
}

/// Temps de réflexion de l'équipe par build.
#[utoipa::path(get, path = "/api/stats/move-times", tag = "statistiques", params(TeamScope), responses(
    (status = 200, body = Vec<MoveTimes>), (status = 400, description = "`team` manquant."),
    (status = 500, description = "Base indisponible."),
))]
async fn stats_move_times(State(db): State<Arc<PathBuf>>, Query(q): Query<TeamScope>) -> Response {
    stat(db, Stat::MoveTimes, q.session, Some(q.team)).await
}

/// Jars soumis par l'équipe et leurs verdicts, du plus ancien au plus récent.
#[utoipa::path(get, path = "/api/stats/submissions", tag = "statistiques", params(Team), responses(
    (status = 200, body = Vec<SubmissionRecord>), (status = 400, description = "`team` manquant."),
    (status = 500, description = "Base indisponible."),
))]
async fn stats_submissions(State(db): State<Arc<PathBuf>>, Query(q): Query<Team>) -> Response {
    stat(db, Stat::Submissions, None, Some(q.team)).await
}

async fn not_found() -> SubmitError {
    SubmitError::NotFound
}

#[derive(Clone)]
struct SubmitState {
    submissions: Arc<Submissions>,
    jobs: Arc<Jobs>,
}

/// How long `GET /api/submit/{id}` keeps a verdict.
const VERDICT_TTL: Duration = Duration::from_secs(3600);

/// Smoke tests started by `PUT /api/submit?async`. shortcut: in memory, a restart forgets them
/// (the poll then gets 404); persist them in the database if that ever matters.
struct Jobs {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Job>>,
}

struct Job {
    team: TeamId,
    verdict: Option<(Instant, StatusCode, String)>,
}

impl Jobs {
    /// Ids start from the clock, so that one from before a restart is not handed out again.
    fn new() -> Self {
        Self {
            next: AtomicU64::new(now_ms()),
            jobs: Mutex::default(),
        }
    }

    fn start(&self, team: TeamId) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        jobs.retain(|_, job| job.verdict.as_ref().is_none_or(|(at, ..)| at.elapsed() < VERDICT_TTL));
        jobs.insert(id, Job { team, verdict: None });
        id
    }

    fn finish(&self, id: u64, status: StatusCode, body: String) {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(job) = jobs.get_mut(&id) {
            job.verdict = Some((Instant::now(), status, body));
        }
    }

    fn verdict(&self, id: u64, team: &TeamId) -> Option<(StatusCode, String)> {
        let jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        let job = jobs.get(&id).filter(|job| &job.team == team)?;
        Some(job.verdict.as_ref().map_or_else(
            || (StatusCode::ACCEPTED, "en cours : smoke test\n".to_owned()),
            |(_, status, body)| (*status, body.clone()),
        ))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

fn outcome(result: Result<Accepted, SubmitError>) -> (StatusCode, String) {
    match result {
        Ok(accepted) => (StatusCode::OK, format!("{accepted}\n")),
        Err(error) => (error.status(), format!("{error}\n")),
    }
}

/// Soumet le jar de l'équipe (corps de la requête) : smoke test, puis installation au ladder.
///
/// Le smoke test peut durer plus que les 100 s qu'un proxy comme Cloudflare laisse à une requête :
/// avec `?async`, la réponse 202 arrive dès la réception du jar et donne le chemin du verdict.
#[utoipa::path(method(put, post), path = "/api/submit", tag = "soumission", security(("token" = [])),
    params(("async" = Option<bool>, Query,
        description = "Présent (`?async`) : répondre 202 tout de suite, verdict sur `GET /api/submit/{id}`.")),
    request_body(content = Jar, content_type = "application/java-archive", description = "Le jar."),
    responses(
        (status = 200, body = String, content_type = "text/plain", description = "Jar installé ou déjà actif."),
        (status = 202, body = String, content_type = "text/plain",
         description = "Avec `?async` : chemin du verdict, aussi dans `Location`.", example = "/api/submit/1762979823004\n"),
        (status = 400, description = "Le corps n'est pas un jar."),
        (status = 401, description = "Token d'équipe absent ou inconnu."),
        (status = 413, description = "Jar trop gros."),
        (status = 422, description = "Refusé par le smoke test ; la raison est dans le corps."),
        (status = 429, description = "Soumission trop rapprochée de la précédente."),
        (status = 500, description = "Erreur du ladder, réessayer plus tard."),
        (status = 503, description = "Ladder en cours d'arrêt."),
    ),
)]
async fn submit(
    State(state): State<SubmitState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, SubmitError> {
    let submissions = state.submissions;
    let team = authenticate(&submissions, &headers).await?;
    let jar = to_bytes(body, submissions.max_body())
        .await
        .map_err(|error| body_error(error, submissions.max_jar_mb()))?;
    if !query.contains_key("async") {
        return Ok(outcome(blocking(move || submissions.submit(&team, &jar)).await).into_response());
    }
    let id = state.jobs.start(team.clone());
    let jobs = state.jobs;
    spawn_blocking(move || {
        let (status, body) = catch_unwind(AssertUnwindSafe(|| submissions.submit(&team, &jar))).map_or_else(
            |_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "erreur du ladder pendant le smoke test, réessayer plus tard\n".to_owned(),
                )
            },
            outcome,
        );
        jobs.finish(id, status, body);
    });
    let path = format!("/api/submit/{id}");
    Ok((StatusCode::ACCEPTED, [(LOCATION, path.clone())], format!("{path}\n")).into_response())
}

/// Verdict d'une soumission `?async`, gardé une heure. Tant que le smoke test tourne : 202 ;
/// ensuite, le code et le texte qu'aurait donnés `PUT /api/submit` sans `?async`.
#[utoipa::path(get, path = "/api/submit/{id}", tag = "soumission", security(("token" = [])),
    params(("id" = String, Path, description = "Id rendu par `PUT /api/submit?async`.")),
    responses(
        (status = 200, body = String, content_type = "text/plain", description = "Jar installé ou déjà actif."),
        (status = 202, body = String, content_type = "text/plain", description = "Smoke test en cours."),
        (status = 401, description = "Token d'équipe absent ou inconnu."),
        (status = 404, description = "Soumission inconnue, d'une autre équipe, ou perdue au redémarrage du ladder."),
    ),
)]
async fn verdict(
    State(state): State<SubmitState>,
    UrlPath(id): UrlPath<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, String), SubmitError> {
    let team = authenticate(&state.submissions, &headers).await?;
    id.parse()
        .ok()
        .and_then(|id| state.jobs.verdict(id, &team))
        .ok_or(SubmitError::UnknownSubmission)
}

async fn authenticate(submissions: &Arc<Submissions>, headers: &HeaderMap) -> Result<TeamId, SubmitError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let submissions = Arc::clone(submissions);
    blocking(move || submissions.authenticate(authorization.as_deref())).await
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
pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    spawn_blocking(work)
        .await
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn a_body_over_the_limit_is_too_large() {
        let limit = 8;
        let error = to_bytes(Body::from(vec![0; limit + 1]), limit).await.unwrap_err();
        assert_eq!(body_error(error, 1).status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
