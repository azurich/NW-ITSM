//! Liveness and database reachability.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// `ok` or `degraded`.
    pub status: &'static str,
    /// `ok` or `unreachable`.
    pub database: &'static str,
    pub version: &'static str,
}

/// Service health, without authentication.
#[utoipa::path(
    get,
    path = "/api/v2/health",
    tag = "system",
    responses(
        (status = 200, description = "Service and database are up", body = Health),
        (status = 503, description = "Database unreachable", body = Health),
    )
)]
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let version = env!("CARGO_PKG_VERSION");
    match nw_db::ping(&state.pool).await {
        Ok(()) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                database: "ok",
                version,
            }),
        ),
        Err(error) => {
            tracing::warn!(%error, "database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "unreachable",
                    version,
                }),
            )
        }
    }
}
