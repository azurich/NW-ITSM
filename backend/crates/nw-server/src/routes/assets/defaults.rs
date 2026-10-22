//! What the classic form of a new item shows before the user changes it,
//! when it depends on the settings: `/assets/{kind}/defaults`.

use axum::{
    Json,
    extract::{Path, State, rejection::PathRejection},
};
use nw_domain::{
    asset::AssetKind,
    php,
    rights::{CREATE, UPDATE},
};
use serde::Serialize;
use utoipa::ToSchema;

use super::AssetType;
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

/// Values of a new item set by the settings; `null` for the other types.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetDefaults {
    /// Software: can be chosen in a ticket
    /// (`default_software_helpdesk_visible`).
    pub is_helpdesk_visible: Option<bool>,
}

/// Values the form of a new item of the type starts with, as the classic
/// form (`getEmpty`) shows them.
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}/defaults",
    tag = "assets",
    params(("kind" = AssetType, Path, description = "Type of asset")),
    responses(
        (status = 200, description = "Values of a new item", body = AssetDefaults),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No create or update right on this type", body = ErrorBody),
    )
)]
pub async fn defaults(
    State(state): State<AppState>,
    session: Session,
    kind: Result<Path<AssetType>, PathRejection>,
) -> ApiResult<Json<AssetDefaults>> {
    let Path(kind) = kind.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    if !(session.rights.has(kind.right(), CREATE) || session.rights.has(kind.right(), UPDATE)) {
        return Err(ApiError::Forbidden);
    }
    let is_helpdesk_visible = if kind == AssetKind::Software {
        Some(software_helpdesk_visible(&state).await?)
    } else {
        None
    };
    Ok(Json(AssetDefaults {
        is_helpdesk_visible,
    }))
}

/// `default_software_helpdesk_visible`, checked when not empty for PHP.
pub(super) async fn software_helpdesk_visible(state: &AppState) -> ApiResult<bool> {
    let name = "default_software_helpdesk_visible";
    let config = nw_db::config::core_values(&state.pool, &[name]).await?;
    Ok(config.get(name).is_some_and(|value| !php::is_empty(value)))
}
