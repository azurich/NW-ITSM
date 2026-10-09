//! `/api/v2` routes.

pub mod auth;
pub mod computers;
pub mod health;

use axum::{Json, Router, routing::get};
use utoipa::OpenApi;

use crate::{openapi::ApiDoc, state::AppState};

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(auth::routes())
        .merge(computers::routes())
        .route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
}
