#![forbid(unsafe_code)]
#![deny(elided_lifetimes_in_paths)]

mod load_env;
mod templates;
mod timer_store;
mod timer_utils;
mod uid;

use std::{env, net::SocketAddr, str::FromStr};

use anyhow::Result;
use askama::Template;
use axum::{
    body::{Bytes, Full},
    debug_handler,
    extract::{Path, State},
    http::{self, header, StatusCode},
    response::{AppendHeaders, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use timer_store::TimerStore;

use timer_utils::export_timers;
use tower::ServiceBuilder;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{debug, error, info, instrument};
use uid::TagId;

pub fn uri_base() -> String {
    let Ok(uri_base) = env::var("URI_BASE") else {
        panic!("URI_BASE not set")
    };

    uri_base
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Load environment variables
    load_env::load_env()?;

    let timer_store = TimerStore::new().await?;
    let state = App { timer_store };
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/timer/:timer_tag", get(timers))
        .route("/timer/:timer_tag/:timezone", get(timers_with_tz))
        .route("/timer/toggle", post(toggle_timer))
        .route("/export/:tag/:timezone", get(export))
        .nest_service("/assets", ServeDir::new("assets/dist"))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    // run our app with hyper, listening globally on port 3000
    let listener = SocketAddr::from_str("0.0.0.0:3000")?;
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

/// Export all finished timers for a tag as a CSV file
#[debug_handler]
async fn export(
    State(app): State<App>,
    Path((filename, timezone)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // Remove the file extension
    let tag = filename.split(".").collect::<Vec<&str>>()[0]
        .to_string()
        .into();
    let timers = app.timer_store.get_exportable_timers_by_tag(&tag).await?;

    let writer = export_timers(timers, &timezone)?;
    let body = Full::new(Bytes::from(writer.into_inner()?));

    let headers = AppendHeaders([(header::CONTENT_TYPE, "text/csv")]);

    Ok((headers, body))
}

// Renders the main timer page for a given tag
#[instrument(skip(app))]
#[debug_handler]
async fn timers(
    State(app): State<App>,
    Path(timer_tag): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    render_timers(app, timer_tag, None).await
}

#[instrument(skip(app))]
#[debug_handler]
async fn timers_with_tz(
    State(app): State<App>,
    Path((timer_tag, timezone)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    render_timers(app, timer_tag, Some(timezone)).await
}

#[instrument(skip(app))]
async fn render_timers(
    app: App,
    timer_tag: String,
    timezone: Option<String>,
) -> Result<Response, AppError> {
    debug!(timer_tag, "Rendering timers");
    let tag = timer_tag.into();
    let timers = app.timer_store.get_timers_by_tag(&tag).await?;

    let rendered_page = templates::render_timers(tag, timezone, timers)?;
    Ok(into_response(&rendered_page))
}

#[derive(Debug, Serialize)]
struct UserContent {
    uid: TagId,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Toggle {
    #[serde(rename = "device-details")]
    pub device_details: String,

    #[serde(rename = "timer-tag")]
    pub timer_tag: String,
}

/// Toggles the current timer for the given tag
#[instrument(skip(app))]
#[debug_handler]
async fn toggle_timer(
    State(app): State<App>,
    Json(toggle): Json<Toggle>,
) -> Result<Json<UserContent>, AppError> {
    info!(tag = ?toggle, "Toggle timer");
    let timer_tag = &toggle.timer_tag;

    let uid = uid::TagId::new(timer_tag)?;

    let id = app.timer_store.toggle_current(&uid).await?;

    debug!(id, message = "Toggled timer");

    Ok(Json(UserContent {
        uid: uid.clone(),
        url: format!("{}/timer/{}", uri_base(), uid.as_ref()),
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

pub fn into_response<T: Template>(t: &T) -> Response {
    match t.render() {
        Ok(body) => {
            let headers = [(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(T::MIME_TYPE),
            )];

            (headers, body).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
