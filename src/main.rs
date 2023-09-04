mod load_env;
mod templates;
mod timer_store;
mod uid;

use std::{net::SocketAddr, str::FromStr};

use anyhow::Result;
use axum::{
    body::{Bytes, Full},
    debug_handler,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{AppendHeaders, Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use csv::WriterBuilder;
use serde::{Deserialize, Serialize};
use timer_store::TimerStore;

use tower::ServiceBuilder;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{debug, error, info, instrument};
use uid::TagId;

use crate::templates::Page;

const LOCAL_URI_BASE: &'static str = "0.0.0.0:3000";

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
        .route("/timer/toggle", post(toggle_timer))
        .route("/export/:tag", get(export))
        .nest_service("/assets", ServeDir::new("assets/dist"))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    // run our app with hyper, listening globally on port 3000
    let listener = SocketAddr::from_str(LOCAL_URI_BASE)?;
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
    Path(tag): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tag = tag.split(".").collect::<Vec<&str>>()[0].to_string().into();
    let timers = app.timer_store.get_exportable_timers_by_tag(&tag).await?;

    let data = vec![];
    let mut writer = WriterBuilder::new().from_writer(data);

    #[derive(Debug, Serialize)]
    struct ExportRecord {
        start_time: String,
        end_time: String,
        duration: i64,
    }

    for timer in timers {
        writer.serialize(ExportRecord {
            start_time: templates::format_time(timer.start_time, "%F %H:%M")?,
            end_time: timer
                .end_time
                .and_then(|time| templates::format_time(time, "fmt_string").ok())
                .unwrap_or(String::new()),
            duration: timer.duration.expect("Non-current timer does not have Duration")
        })?;
    }
    writer.flush()?;
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
    debug!(timer_tag, "Rendering timers");
    let tag = timer_tag.into();
    let timers = app.timer_store.get_timers_by_tag(&tag).await?;

    let file_name = format!("{}.csv", tag.as_ref());
    let link = format!("http://{}/export/{}", LOCAL_URI_BASE, file_name);

    Ok(Html(templates::render_timers(Page::new(
        tag.as_ref().to_string(),
        timers,
        link,
        file_name,
    )?)?))
}

#[derive(Debug, Serialize)]
struct UserContent {
    uid: TagId,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Toggle {
    #[serde(rename = "device-time")]
    pub device_time: String,

    #[serde(rename = "timer-tag")]
    pub timer_tag: String,
}

/// Toggles the current timer for the given tag
#[instrument(skip_all)]
#[debug_handler]
async fn toggle_timer(
    State(app): State<App>,
    Json(toggle): Json<Toggle>,
) -> Result<Json<UserContent>, AppError> {
    let timer_tag = &toggle.timer_tag;
    info!(tag = ?toggle, "Toggle timer");

    let uid = uid::TagId::new(timer_tag)?;

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
