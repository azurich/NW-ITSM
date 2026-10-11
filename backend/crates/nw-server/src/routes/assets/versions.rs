//! Versions of a software, under `/assets/softwares/{id}/versions`: added,
//! changed and purged like with the legacy form of a version
//! (`front/softwareversion.form.php`), with the rights of their software.

use axum::{
    Json,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
};
use nw_db::assets::{self, versions};
use nw_domain::{
    asset::{AssetKind, Value},
    history,
    i18n::{self, Source, text},
    rights::{READ, UPDATE},
    software::version_history_name,
    text::to_stored,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    MAX_LONG_TEXT_BYTES, MAX_TEXT_CHARS, Ref, decode, log_asset_event, nullable_id, nullable_text,
};
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

/// A version of a software.
#[derive(Debug, Serialize, ToSchema)]
pub struct Version {
    pub id: i32,
    pub name: Option<String>,
    pub comment: Option<String>,
    /// The entity of its software.
    pub entity: Ref,
    /// Follows the sharing of its software.
    pub is_recursive: bool,
    pub operating_system: Option<Ref>,
    pub state: Option<Ref>,
    /// Items it is installed on, outside the trash.
    pub installations: i64,
}

impl From<versions::VersionRow> for Version {
    fn from(row: versions::VersionRow) -> Self {
        Self {
            id: row.id,
            name: decode(row.name),
            comment: decode(row.comment),
            entity: Ref {
                id: row.entities_id,
                name: decode(row.entity),
            },
            is_recursive: row.is_recursive,
            operating_system: Ref::optional(row.operatingsystems_id, row.operating_system),
            state: Ref::optional(row.states_id, row.state),
            installations: row.installations,
        }
    }
}

/// The versions of a software.
#[derive(Debug, Serialize, ToSchema)]
pub struct VersionList {
    /// By name.
    pub items: Vec<Version>,
    /// The session may add, change and purge them (update right on the
    /// software, in one of its entities).
    pub can_edit: bool,
}

/// Fields of a version. Absent fields are left unchanged (empty for a new
/// version); `null` clears a field.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct VersionFields {
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub comment: Option<String>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub operating_system_id: Option<i32>,
    /// A status offered for the versions (`is_visible_softwareversion`).
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub state_id: Option<i32>,
}

/// Lists the versions of a software visible to the session, by name.
#[utoipa::path(
    get,
    path = "/api/v2/assets/softwares/{id}/versions",
    operation_id = "list_versions",
    tag = "assets",
    params(("id" = i32, Path, description = "Software id")),
    responses(
        (status = 200, description = "Its versions", body = VersionList),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on the software", body = ErrorBody),
        (status = 404, description = "Unknown or not visible software", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<i32>, PathRejection>,
) -> ApiResult<Json<VersionList>> {
    let Path(id) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    session.require(AssetKind::Software.right(), READ)?;
    let software = visible(&state, &session, id).await?;
    let rows = versions::list(&state.pool, id).await?;
    Ok(Json(VersionList {
        items: rows.into_iter().map(Version::from).collect(),
        can_edit: session.rights.has(AssetKind::Software.right(), UPDATE)
            && session.scope.active().contains(&software.entities_id),
    }))
}

/// Adds a version to a software, in its entity and with its sharing. The
/// software notes it in its history.
#[utoipa::path(
    post,
    path = "/api/v2/assets/softwares/{id}/versions",
    operation_id = "create_version",
    tag = "assets",
    params(("id" = i32, Path, description = "Software id")),
    request_body = VersionFields,
    responses(
        (status = 201, description = "Added", body = Version),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No update right on the software, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible software", body = ErrorBody),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<i32>, PathRejection>,
    payload: Result<Json<VersionFields>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Version>)> {
    let Path(id) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let software = writable(&state, &session, id).await?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let values = values(&state, &fields).await?;
    let actor = actor(&session);
    let version_id = versions::create(
        &state.pool,
        &software,
        &values,
        &actor,
        &name_version(&session),
    )
    .await?;
    log_version_event(&state, &session, id, text::ADDS_VERSION, version_id).await;
    let row = versions::get(&state.pool, id, version_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// Changes a version: its history, and that of its software.
#[utoipa::path(
    patch,
    path = "/api/v2/assets/softwares/{id}/versions/{version_id}",
    operation_id = "update_version",
    tag = "assets",
    params(
        ("id" = i32, Path, description = "Software id"),
        ("version_id" = i32, Path, description = "Version id"),
    ),
    request_body = VersionFields,
    responses(
        (status = 200, description = "Updated", body = Version),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No update right on the software, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible software or version", body = ErrorBody),
    )
)]
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(i32, i32)>, PathRejection>,
    payload: Result<Json<VersionFields>, JsonRejection>,
) -> ApiResult<Json<Version>> {
    let Path((id, version_id)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let software = writable(&state, &session, id).await?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let values = values(&state, &fields).await?;
    versions::update(
        &state.pool,
        &software,
        version_id,
        &values,
        &actor(&session),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    // The form logs it, whatever changed.
    log_version_event(&state, &session, id, text::UPDATES_VERSION, version_id).await;
    let row = versions::get(&state.pool, id, version_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row.into()))
}

/// Deletes a version for good, with its installations (without history).
/// Licenses bought or used with it leave it to the classic interface.
#[utoipa::path(
    delete,
    path = "/api/v2/assets/softwares/{id}/versions/{version_id}",
    operation_id = "purge_version",
    tag = "assets",
    params(
        ("id" = i32, Path, description = "Software id"),
        ("version_id" = i32, Path, description = "Version id"),
    ),
    responses(
        (status = 204, description = "Deleted for good"),
        (status = 403, description = "No update right on the software, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible software or version", body = ErrorBody),
        (status = 409, description = "Must be done in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn purge(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(i32, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let Path((id, version_id)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let software = writable(&state, &session, id).await?;
    versions::get(&state.pool, id, version_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !versions::purge_blockers(&state.pool, version_id)
        .await?
        .is_empty()
    {
        return Err(ApiError::LegacyRequired("linked_data"));
    }
    if !versions::purge(
        &state.pool,
        &software,
        version_id,
        &actor(&session),
        &name_version(&session),
    )
    .await?
    {
        return Err(ApiError::NotFound);
    }
    log_version_event(&state, &session, id, text::PURGES_VERSION, version_id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// The software, if visible to the session.
async fn visible(state: &AppState, session: &Session, id: i32) -> ApiResult<assets::Detail> {
    assets::get(&state.pool, AssetKind::Software, &session.scope, id, false)
        .await?
        .ok_or(ApiError::NotFound)
}

/// The software whose versions the session may change: the update right
/// on the software (`CommonDBChild`), in one of the selected entities.
async fn writable(state: &AppState, session: &Session, id: i32) -> ApiResult<versions::Software> {
    session.require(AssetKind::Software.right(), UPDATE)?;
    let software = visible(state, session, id).await?;
    if !session.scope.active().contains(&software.entities_id) {
        return Err(ApiError::Forbidden);
    }
    Ok(versions::Software {
        id,
        name: software.name,
        entities_id: software.entities_id,
        is_recursive: software.is_recursive,
    })
}

/// The given fields, validated, as stored.
async fn values(state: &AppState, fields: &VersionFields) -> ApiResult<Vec<(&'static str, Value)>> {
    let mut values = Vec::new();
    for (name, column, text, max) in [
        ("name", "name", &fields.name, None),
        (
            "comment",
            "comment",
            &fields.comment,
            Some(MAX_LONG_TEXT_BYTES),
        ),
    ] {
        let Some(text) = text else { continue };
        let stored = to_stored(text);
        let too_long = match max {
            Some(bytes) => stored.len() > bytes,
            None => stored.chars().count() > MAX_TEXT_CHARS,
        };
        if too_long {
            return Err(ApiError::BadRequest(format!("{name}: too long")));
        }
        values.push((column, Value::Text(Some(stored))));
    }
    for (name, column, id) in [
        (
            "operating_system_id",
            "operatingsystems_id",
            fields.operating_system_id,
        ),
        ("state_id", "states_id", fields.state_id),
    ] {
        let Some(id) = id else { continue };
        if id < 0 {
            return Err(ApiError::BadRequest(format!("{name}: invalid id")));
        }
        values.push((column, Value::Id(id)));
    }
    if !versions::references_exist(&state.pool, &values).await? {
        return Err(ApiError::BadRequest("unknown referenced value".to_owned()));
    }
    Ok(values)
}

/// Names a version in the history of its software, in the language of
/// the author.
fn name_version(session: &Session) -> impl Fn(Option<&str>, Option<&str>, i32) -> String + Sync {
    let dash = i18n::tr(&session.preferences.language, text::DASH);
    let name_id = i18n::tr(&session.preferences.language, text::NAME_ID);
    move |software, version, id| version_history_name(&dash, &name_id, software, version, id)
}

/// Author of the history entries.
fn actor(session: &Session) -> String {
    history::actor(session.user.id, &session.own_name())
}

/// Event of the version form: on the software, with the id of the version.
async fn log_version_event(
    state: &AppState,
    session: &Session,
    software_id: i32,
    action: Source,
    version_id: i32,
) {
    log_asset_event(
        state,
        session,
        AssetKind::Software,
        software_id,
        action,
        &[&version_id.to_string()],
    )
    .await;
}
