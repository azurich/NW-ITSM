//! `/api/v2` routes.

pub mod assets;
pub mod auth;
pub mod health;
pub mod session;

use axum::{Json, Router, routing::get};
use utoipa::OpenApi;

use crate::{openapi::ApiDoc, state::AppState};

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(auth::routes())
        .merge(session::routes())
        .merge(assets::routes())
        .route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
}
