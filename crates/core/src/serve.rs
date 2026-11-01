//! Read-only HTTP API over the dataset (axum).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::error::Result;
use crate::model::Stats;
use crate::store::{Db, ProjectQuery};

pub struct AppState {
    pub db: Db,
}

/// Build the API router.
pub fn router(db: Db) -> Router {
    Router::new()
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{id}", get(get_project))
        .route("/api/events", get(list_events))
        .route("/api/stats", get(get_stats))
        .route("/healthz", get(health))
        .with_state(Arc::new(AppState { db }))
}

/// Serve the API.
pub async fn serve(db: Db, addr: SocketAddr) -> Result<()> {
    let app = router(db);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize, Debug, Default)]
pub struct ListParams {
    q: Option<String>,
    event: Option<String>,
    year: Option<i64>,
    finalist: Option<bool>,
    prize: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn list_projects(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
    let query = ProjectQuery {
        q: params.q,
        event: params.event,
        year: params.year,
        finalist: params.finalist,
        prize: params.prize,
        limit: params.limit.unwrap_or(50),
        offset: params.offset.unwrap_or(0),
    };
    match state.db.query_projects(&query) {
        Ok((projects, total)) => (
            [(header::CACHE_CONTROL, "public, max-age=300")],
            Json(json!({ "total": total, "count": projects.len(), "items": projects })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_project(&id) {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": "project not found" })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

async fn list_events(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_events() {
        Ok(events) => Json(events).into_response(),
        Err(e) => error_response(e),
    }
}

async fn get_stats(State(state): State<Arc<AppState>>) -> Json<Stats> {
    Json(state.db.stats().unwrap_or_default())
}

async fn health() -> &'static str {
    "ok"
}

fn error_response(e: crate::error::Error) -> axum::response::Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() })),
    )
        .into_response()
}
