mod load_env;
mod timer_store;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
};

use anyhow::Result;
use axum::{
    debug_handler,
    extract::State,
    headers::UserAgent,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router, TypedHeader,
};
use serde::Serialize;
use timer_store::TimerStore;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{debug, info, instrument};

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    load_env::load_env()?;

    let timer_store = TimerStore::new().await?;
    let state = App { timer_store };
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/timer/toggle", get(toggle_timer))
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
async fn root() -> &'static str {
    "Hello, World!"
}

#[derive(Debug, Serialize)]
struct UserContent {
    uid: i64,
    url: String,
}

#[instrument(skip(app))]
#[debug_handler]
async fn toggle_timer(
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    State(app): State<App>,
    headers: HeaderMap,
) -> Result<Json<UserContent>, AppError> {
    let timer_tag = headers
        .get("x-timer-tag")
        .ok_or(anyhow::anyhow!("Timer tag header was not found"))?;
    let mut hasher = DefaultHasher::new();
    user_agent.as_str().hash(&mut hasher);
    timer_tag.hash(&mut hasher);
    let uid = hasher.finish();

    info!(uid, message = "Toggelling timer");
    let id = app.timer_store.create_timer(uid.try_into()?).await?;

    debug!(id, message = "Created timer");

    Ok(Json(UserContent {
        uid: uid.try_into()?,
        url: "https://url-here".to_string(),
    }))
}

// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
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
        Self(err.into())
    }
}