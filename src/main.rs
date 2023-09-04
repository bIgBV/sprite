mod load_env;
mod templates;
mod timer_store;
mod uid;

use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    debug_handler,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use timer_store::TimerStore;
use tower::ServiceBuilder;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{debug, error, info, instrument};
use uid::TagId;

use crate::templates::Page;

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Load environment variables
    load_env::load_env()?;

    // Initialize templates
    templates::init_templates();

    let timer_store = TimerStore::new().await?;
    let state = App { timer_store };
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/timer/:timer_tag", get(timers))
        .route("/timer/toggle", get(toggle_timer))
        .nest_service("/assets", ServeDir::new("assets/dist"))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    // run our app with hyper, listening globally on port 3000
    let listener = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("listening on {}", listener);
    axum::Server::bind(&listener)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

#[derive(Debug, Clone)]
pub struct App {
    timer_store: TimerStore,
}

// basic handler that responds with a static string
#[instrument(skip(app))]
#[debug_handler]
async fn timers(
    State(app): State<App>,
    Path(timer_tag): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    debug!(timer_tag, "Rendering timers");
    let tag = timer_tag.into();
    let timers = app.timer_store.get_timers_by_tag(&tag).await?;

    Ok(Html(templates::render_timers(Page::new(
        tag.as_ref().to_string(),
        timers,
    ))?))
}

#[derive(Debug, Serialize)]
struct UserContent {
    uid: TagId,
    url: String,
}

#[instrument(skip_all)]
#[debug_handler]
async fn toggle_timer(
    State(app): State<App>,
    headers: HeaderMap,
) -> Result<Json<UserContent>, AppError> {
    let timer_tag = headers
        .get("x-timer-tag")
        .ok_or(anyhow::anyhow!("Timer tag header was not found"))?;
    info!(tag = ?timer_tag, "Toggle timer");

    let uid = uid::TagId::new(timer_tag.to_str()?)?;

    let id = app.timer_store.toggle_current(&uid).await?;

    debug!(id, message = "Toggled timer");

    Ok(Json(UserContent {
        uid: uid.clone(),
        url: format!("http://192.168.1.12:3000/timer/{}", uid.as_ref()),
    }))
}

// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(error = %self.0, "backtrace: {}", self.0.backtrace());
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        let into = err.into();
        Self(into)
    }
}
