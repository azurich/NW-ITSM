//! Software installed on an asset, under `/assets/{type}/{id}/software`:
//! the software tab of the classic interface (`Item_SoftwareVersion`),
//! with its installation form and its "Delete permanently" action.

use axum::{
    Json,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
};
use chrono::NaiveDate;
use nw_db::{
    assets::{self, installations},
    events,
};
use nw_domain::{
    asset::AssetKind,
    entity::EntitySelection,
    history,
    i18n::{self, text},
    rights::{CREATE, DELETE, PURGE, READ, UPDATE},
    software::version_history_name,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{AssetType, Ref, decode};
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    routes::auth::log_event,
    session::Session,
    state::AppState,
};

/// A software installed on the asset.
#[derive(Debug, Serialize, ToSchema)]
pub struct Installation {
    pub id: i32,
    pub software: Option<Ref>,
    pub version: Ref,
    /// Status of the version.
    pub version_state: Option<String>,
    pub category: Option<Ref>,
    /// The licenses of the software are valid.
    pub valid_licenses: Option<bool>,
    /// Date of installation, given by the inventory.
    pub installed_on: Option<NaiveDate>,
    /// Installed by the inventory.
    pub is_dynamic: bool,
}

impl From<installations::InstallationRow> for Installation {
    fn from(row: installations::InstallationRow) -> Self {
        Self {
            id: row.id,
            software: row
                .softwares_id
                .and_then(|id| Ref::optional(id, row.software)),
            version: Ref {
                id: row.softwareversions_id,
                name: decode(row.version),
            },
            version_state: decode(row.version_state),
            category: row
                .softwarecategories_id
                .and_then(|id| Ref::optional(id, row.category)),
            valid_licenses: row.is_valid,
            installed_on: row.date_install,
            is_dynamic: row.is_dynamic,
        }
    }
}

/// The software installed on an asset.
#[derive(Debug, Serialize, ToSchema)]
pub struct InstallationList {
    /// Outside the trash, of the software visible to the session, by
    /// software and version.
    pub items: Vec<Installation>,
    /// The software tab offers to install and uninstall: a right on the
    /// software, the asset in one of the selected entities. Each version
    /// is still checked.
    pub can_edit: bool,
}

/// A version to install.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InstallationFields {
    pub version_id: i32,
}

/// Lists the software installed on an asset.
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}/{id}/software",
    operation_id = "list_installations",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset: computers, monitors, network-equipments, peripherals, phones or printers"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 200, description = "Its software", body = InstallationList),
        (status = 400, description = "No software for this type", body = ErrorBody),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on the software or on the asset", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<Json<InstallationList>> {
    let (kind, id) = parse(path)?;
    session.require(AssetKind::Software.right(), READ)?;
    session.require(kind.right(), READ)?;
    let asset = visible(&state, &session, kind, id).await?;
    let rows = installations::list(&state.pool, kind, id, &session.scope).await?;
    let software = AssetKind::Software.right();
    let can_edit = [CREATE, UPDATE, DELETE, PURGE]
        .into_iter()
        .any(|right| session.rights.has(software, right))
        && session.scope.active().contains(&asset.entities_id);
    Ok(Json(InstallationList {
        items: rows.into_iter().map(Installation::from).collect(),
        can_edit,
    }))
}

/// Installs a version of a software on an asset, like the form of its
/// software tab: in the entity of the asset (the active entity of the
/// session for a phone), noted in the history of both.
#[utoipa::path(
    post,
    path = "/api/v2/assets/{kind}/{id}/software",
    operation_id = "install_software",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    request_body = InstallationFields,
    responses(
        (status = 201, description = "Installed", body = Installation),
        (status = 400, description = "Invalid fields, unknown version, or no software for this type", body = ErrorBody),
        (status = 403, description = "Neither the asset nor the software can be changed, or their entities do not match", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset", body = ErrorBody),
        (status = 409, description = "Already installed (`already_installed`)", body = ErrorBody),
    )
)]
pub async fn install(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    payload: Result<Json<InstallationFields>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Installation>)> {
    let (kind, id) = parse(path)?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    // `check(-1, CREATE)`: the static right of the relation is the view of
    // the software.
    session.require(AssetKind::Software.right(), READ)?;
    let asset = visible(&state, &session, kind, id).await?;
    let version = installations::version(&state.pool, fields.version_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("version_id: unknown id".to_owned()))?;
    if !may_change(&session, kind, &asset, &version) || !entities_match(&session, &asset, &version)
    {
        return Err(ApiError::Forbidden);
    }
    let entity = if kind.forwards_entity_to_installations() {
        asset.entities_id
    } else {
        active_entity(&session)
    };
    let target = target(kind, &asset);
    let installation = installations::install(
        &state.pool,
        &target,
        version.id,
        entity,
        &actor(&session),
        &history_names(&session, &asset, &version),
    )
    .await?
    .ok_or(ApiError::AlreadyInstalled)?;
    log_install_event(&state, &session, kind, id).await;
    let row = installations::list(&state.pool, kind, id, &session.scope)
        .await?
        .into_iter()
        .find(|row| row.id == installation)
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// Uninstalls a software from an asset, for good, like the "Delete
/// permanently" action of its software tab (no event), noted in the
/// history of the asset and of the version.
#[utoipa::path(
    delete,
    path = "/api/v2/assets/{kind}/{id}/software/{installation_id}",
    operation_id = "uninstall_software",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
        ("installation_id" = i32, Path, description = "Installation id"),
    ),
    responses(
        (status = 204, description = "Uninstalled"),
        (status = 400, description = "No software for this type", body = ErrorBody),
        (status = 403, description = "Neither the asset nor the software can be changed", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset or installation", body = ErrorBody),
    )
)]
pub async fn uninstall(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let Path((kind, id, installation)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = software_kind(kind.into())?;
    session.require(AssetKind::Software.right(), READ)?;
    let asset = visible(&state, &session, kind, id).await?;
    let version_id = installations::installed_version(&state.pool, kind, id, installation)
        .await?
        .ok_or(ApiError::NotFound)?;
    // A version or a software that no longer exists: the PHP code refuses.
    let version = installations::version(&state.pool, version_id)
        .await?
        .ok_or(ApiError::Forbidden)?;
    if !may_change(&session, kind, &asset, &version) {
        return Err(ApiError::Forbidden);
    }
    let uninstalled = installations::uninstall(
        &state.pool,
        &target(kind, &asset),
        installation,
        &actor(&session),
        &history_names(&session, &asset, &version),
    )
    .await?;
    if !uninstalled {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn parse(path: Result<Path<(AssetType, i32)>, PathRejection>) -> ApiResult<(AssetKind, i32)> {
    let Path((kind, id)) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    Ok((software_kind(kind.into())?, id))
}

/// The types with a software tab (`$CFG_GLPI['software_types']`).
fn software_kind(kind: AssetKind) -> ApiResult<AssetKind> {
    if kind.has_software() {
        Ok(kind)
    } else {
        Err(ApiError::BadRequest(format!(
            "no software for {}",
            kind.slug()
        )))
    }
}

/// The asset, outside the templates, if visible to the session.
async fn visible(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
) -> ApiResult<assets::Detail> {
    assets::get(&state.pool, kind, &session.scope, id, false)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Whether the session may install or uninstall the version on the asset
/// (`CommonDBRelation::canRelationItem`): one side must be writable and
/// the other visible. The asset is writable with its update right in one
/// of the selected entities, visible with its read right; the version
/// follows its software, writable with the update right on the software
/// in one of the selected entities, visible when shared with them.
///
/// Deliberate difference: the PHP code checks only the entity of the
/// software, not the update right, so a read-only profile can install a
/// software there; the API refuses it.
fn may_change(
    session: &Session,
    kind: AssetKind,
    asset: &assets::Detail,
    version: &installations::Version,
) -> bool {
    let scope = &session.scope;
    let asset_writable =
        session.rights.has(kind.right(), UPDATE) && scope.active().contains(&asset.entities_id);
    // Visible to the session: it was read in its scope.
    let asset_visible = session.rights.has(kind.right(), READ);
    let software_in_scope = scope.active().contains(&version.software_entities_id);
    let software_writable =
        session.rights.has(AssetKind::Software.right(), UPDATE) && software_in_scope;
    // The read right on the software is checked before.
    let software_visible = software_in_scope
        || (version.software_is_recursive
            && scope.parents().contains(&version.software_entities_id));
    (asset_writable || software_writable)
        && (!asset_writable || software_visible)
        && (!software_writable || asset_visible)
}

/// The asset and the version are in the same entity, or one is shared with
/// the entity of the other (`$check_entity_coherency`).
fn entities_match(
    session: &Session,
    asset: &assets::Detail,
    version: &installations::Version,
) -> bool {
    let (asset_entity, version_entity) = (asset.entities_id, version.entities_id);
    asset_entity == version_entity
        || (asset.is_recursive
            && session
                .tree
                .ancestors_of(version_entity)
                .contains(&asset_entity))
        || (version.is_recursive
            && session
                .tree
                .ancestors_of(asset_entity)
                .contains(&version_entity))
}

/// The active entity of the classic interface (`glpiactive_entity`): the
/// selected one, or the first entity of the profile by full name when
/// they are all selected.
fn active_entity(session: &Session) -> i32 {
    match session.selection {
        EntitySelection::Entity { id, .. } => id,
        EntitySelection::All => session
            .granted
            .first()
            .map_or(0, |granted| granted.entity_id),
    }
}

fn target(kind: AssetKind, asset: &assets::Detail) -> installations::Asset {
    installations::Asset {
        kind,
        id: asset.id,
        is_deleted: asset.is_deleted,
        is_template: asset.is_template != 0,
    }
}

/// Each side as named in the history of the other, in the language of the
/// author (`getNameID` with the id).
fn history_names(
    session: &Session,
    asset: &assets::Detail,
    version: &installations::Version,
) -> installations::HistoryNames {
    let language = &session.preferences.language;
    let name_id = i18n::tr(language, text::NAME_ID);
    let dash = i18n::tr(language, text::DASH);
    installations::HistoryNames {
        version: version_history_name(
            &dash,
            &name_id,
            version.software.as_deref(),
            version.name.as_deref(),
            version.id,
        ),
        asset: history::name_id(&name_id, asset.name.as_deref(), asset.id),
    }
}

fn actor(session: &Session) -> String {
    history::actor(session.user.id, &session.own_name())
}

/// Event of the installation form: on the asset, its class as type.
async fn log_install_event(state: &AppState, session: &Session, kind: AssetKind, id: i32) {
    let login = session.user.name.clone().unwrap_or_default();
    let format = i18n::tr(&session.preferences.language, text::INSTALLS_SOFTWARE);
    log_event(
        state,
        &events::Event {
            items_id: id,
            kind: kind.itemtype(),
            service: "inventory",
            level: 5,
            message: &i18n::sprintf(&format, &[&login]),
        },
    )
    .await;
}
