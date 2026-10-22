//! Affectations of the licenses to the items with software
//! (`Item_SoftwareLicense`), from the license
//! (`/assets/software-licenses/{id}/items`) and from the item
//! (`/assets/{type}/{id}/licenses`): the form of the classic interface
//! adds them, the "Delete permanently" action of their tabs removes them.

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
    assets::{self, licenses},
    events,
};
use nw_domain::{
    asset::AssetKind,
    history,
    i18n::{self, text},
    rights::{CREATE, DELETE, PURGE, READ, UPDATE},
    rules::Operation,
    software::version_history_name,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{AssetType, Ref, business, decode};
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    routes::auth::log_event,
    session::Session,
    state::AppState,
};

const LICENSE: AssetKind = AssetKind::SoftwareLicense;

/// An item a license is affected to.
#[derive(Debug, Serialize, ToSchema)]
pub struct LicenseItem {
    /// Id of the affectation.
    pub id: i32,
    pub kind: AssetType,
    pub item: Ref,
    pub entity: Ref,
    pub serial: Option<String>,
    pub inventory_number: Option<String>,
    /// The item is in the trash.
    pub is_deleted: bool,
}

/// The items a license is affected to.
#[derive(Debug, Serialize, ToSchema)]
pub struct LicenseItemList {
    /// Visible to the session, by entity and name.
    pub items: Vec<LicenseItem>,
    /// The tab offers to add and remove items (a right on the software).
    pub can_edit: bool,
}

/// A license affected to an item.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetLicense {
    /// Id of the affectation.
    pub id: i32,
    pub license: Ref,
    pub software: Option<Ref>,
    pub number: i32,
    pub is_valid: bool,
    pub expire: Option<NaiveDate>,
}

/// The licenses affected to an item.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetLicenseList {
    /// Visible to the session, by software and name.
    pub items: Vec<AssetLicense>,
    /// The tab offers to add and remove licenses (a right on the
    /// software, the item in one of the selected entities).
    pub can_edit: bool,
}

/// An item to affect a license to.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LicenseItemFields {
    /// A type with software: computers, monitors, network equipment,
    /// peripherals, phones, printers.
    pub kind: AssetType,
    pub item_id: i32,
}

/// A license to affect to an item.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AssetLicenseFields {
    pub license_id: i32,
}

/// Lists the items a license is affected to.
#[utoipa::path(
    get,
    path = "/api/v2/assets/software-licenses/{id}/items",
    operation_id = "list_license_items",
    tag = "assets",
    params(("id" = i32, Path, description = "License id")),
    responses(
        (status = 200, description = "Its items", body = LicenseItemList),
        (status = 403, description = "No read right on the software or the licenses", body = ErrorBody),
        (status = 404, description = "Unknown or not visible license", body = ErrorBody),
    )
)]
pub async fn license_items(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<i32>, PathRejection>,
) -> ApiResult<Json<LicenseItemList>> {
    let Path(id) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    session.require(AssetKind::Software.right(), READ)?;
    session.require(LICENSE.right(), READ)?;
    visible(&state, &session, LICENSE, id).await?;
    let rows = licenses::affected_items(&state.pool, id, &session.scope).await?;
    let items = rows
        .into_iter()
        .filter_map(|row| {
            let kind = AssetKind::from_itemtype(&row.itemtype)?;
            Some(LicenseItem {
                id: row.id,
                kind: kind.into(),
                item: Ref {
                    id: row.items_id,
                    name: decode(row.name),
                },
                entity: Ref {
                    id: row.entities_id,
                    name: decode(row.entity),
                },
                serial: decode(row.serial),
                inventory_number: decode(row.otherserial),
                is_deleted: row.is_deleted,
            })
        })
        .collect();
    Ok(Json(LicenseItemList {
        items,
        can_edit: tab_editable(&session),
    }))
}

/// Affects a license to an item, like the form of its tab: its validity
/// and that of its software follow, both are noted in their history.
#[utoipa::path(
    post,
    path = "/api/v2/assets/software-licenses/{id}/items",
    operation_id = "add_license_item",
    tag = "assets",
    params(("id" = i32, Path, description = "License id")),
    request_body = LicenseItemFields,
    responses(
        (status = 201, description = "Affected"),
        (status = 400, description = "A type without software", body = ErrorBody),
        (status = 403, description = "No update right on the software", body = ErrorBody),
        (status = 404, description = "Unknown or not visible license or item", body = ErrorBody),
        (status = 409, description = "Business rules would change the license (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn add_license_item(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<i32>, PathRejection>,
    payload: Result<Json<LicenseItemFields>, JsonRejection>,
) -> ApiResult<StatusCode> {
    let Path(id) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    affect(
        &state,
        &session,
        id,
        software_kind(fields.kind.into())?,
        fields.item_id,
    )
    .await?;
    Ok(StatusCode::CREATED)
}

/// Removes an item from a license for good, like the "Delete
/// permanently" action of its tab (no event).
#[utoipa::path(
    delete,
    path = "/api/v2/assets/software-licenses/{id}/items/{affectation_id}",
    operation_id = "remove_license_item",
    tag = "assets",
    params(
        ("id" = i32, Path, description = "License id"),
        ("affectation_id" = i32, Path, description = "Affectation id"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, description = "Neither the item nor the license can be changed", body = ErrorBody),
        (status = 404, description = "Unknown or not visible license or affectation", body = ErrorBody),
        (status = 409, description = "Business rules would change the license (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn remove_license_item(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(i32, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let Path((id, affectation)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let found = licenses::affectation(&state.pool, affectation)
        .await?
        .filter(|found| found.license == id)
        .ok_or(ApiError::NotFound)?;
    unaffect(&state, &session, affectation, found).await
}

/// Lists the licenses affected to an item.
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}/{id}/licenses",
    operation_id = "list_asset_licenses",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset, with software"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 200, description = "Its licenses", body = AssetLicenseList),
        (status = 400, description = "A type without software", body = ErrorBody),
        (status = 403, description = "No read right on the software or the asset", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset", body = ErrorBody),
    )
)]
pub async fn asset_licenses(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<Json<AssetLicenseList>> {
    let Path((kind, id)) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = software_kind(kind.into())?;
    session.require(AssetKind::Software.right(), READ)?;
    session.require(kind.right(), READ)?;
    let asset = visible(&state, &session, kind, id).await?;
    let rows = licenses::asset_licenses(&state.pool, kind, id, &session.scope).await?;
    Ok(Json(AssetLicenseList {
        items: rows
            .into_iter()
            .map(|row| AssetLicense {
                id: row.id,
                license: Ref {
                    id: row.license_id,
                    name: decode(row.name),
                },
                software: Ref::optional(row.softwares_id, row.software),
                number: row.number,
                is_valid: row.is_valid,
                expire: row.expire,
            })
            .collect(),
        can_edit: tab_editable(&session) && session.scope.active().contains(&asset.entities_id),
    }))
}

/// Affects a license to an item, from the item.
#[utoipa::path(
    post,
    path = "/api/v2/assets/{kind}/{id}/licenses",
    operation_id = "add_asset_license",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset, with software"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    request_body = AssetLicenseFields,
    responses(
        (status = 201, description = "Affected"),
        (status = 400, description = "A type without software", body = ErrorBody),
        (status = 403, description = "No update right on the software", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset or license", body = ErrorBody),
        (status = 409, description = "Business rules would change the license (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn add_asset_license(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    payload: Result<Json<AssetLicenseFields>, JsonRejection>,
) -> ApiResult<StatusCode> {
    let Path((kind, id)) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    affect(
        &state,
        &session,
        fields.license_id,
        software_kind(kind.into())?,
        id,
    )
    .await?;
    Ok(StatusCode::CREATED)
}

/// Removes a license from an item for good.
#[utoipa::path(
    delete,
    path = "/api/v2/assets/{kind}/{id}/licenses/{affectation_id}",
    operation_id = "remove_asset_license",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset, with software"),
        ("id" = i32, Path, description = "Asset id"),
        ("affectation_id" = i32, Path, description = "Affectation id"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, description = "Neither the item nor the license can be changed", body = ErrorBody),
        (status = 404, description = "Unknown or not visible asset or affectation", body = ErrorBody),
        (status = 409, description = "Business rules would change the license (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn remove_asset_license(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let Path((kind, id, affectation)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = software_kind(kind.into())?;
    let found = licenses::affectation(&state.pool, affectation)
        .await?
        .filter(|found| found.kind == kind && found.item == id)
        .ok_or(ApiError::NotFound)?;
    unaffect(&state, &session, affectation, found).await
}

/// The tabs offer their forms and actions with a right on the software.
fn tab_editable(session: &Session) -> bool {
    let software = AssetKind::Software.right();
    [CREATE, UPDATE, DELETE, PURGE]
        .into_iter()
        .any(|right| session.rights.has(software, right))
}

/// The types with software (`$CFG_GLPI['software_types']`).
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

/// An item (or license), outside the templates, if visible to the session.
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

/// The form of the classic interface (`front/item_softwarelicense.form.php`)
/// checks only the update right on the software; the API also requires
/// the license and the item to be visible to the session.
async fn affect(
    state: &AppState,
    session: &Session,
    license: i32,
    kind: AssetKind,
    item: i32,
) -> ApiResult<()> {
    session.require(AssetKind::Software.right(), UPDATE)?;
    session.require(LICENSE.right(), READ)?;
    session.require(kind.right(), READ)?;
    let license_row = visible(state, session, LICENSE, license).await?;
    let item_row = visible(state, session, kind, item).await?;
    check_rules(state, session, license, kind, item, true).await?;
    let names = relation_names(state, session, &license_row, &item_row).await?;
    let actor = history::actor(session.user.id, &session.own_name());
    licenses::affect(&state.pool, license, kind, item, &actor, &names).await?;
    // Event of the form, on the license.
    let login = session.user.name.clone().unwrap_or_default();
    let format = i18n::tr(&session.preferences.language, text::ASSOCIATES_LICENSE);
    log_event(
        state,
        &events::Event {
            items_id: license,
            kind: "softwarelicense",
            service: "inventory",
            level: 4,
            message: &i18n::sprintf(&format, &[&login]),
        },
    )
    .await;
    Ok(())
}

/// The purge of an affectation (`canPurgeItem` of the relation): one side
/// writable, the other visible. The license is writable with the update
/// right on the licenses in one of the selected entities (the PHP code
/// checks only its entity), the item with its update right.
async fn unaffect(
    state: &AppState,
    session: &Session,
    id: i32,
    affectation: licenses::Affectation,
) -> ApiResult<StatusCode> {
    session.require(LICENSE.right(), READ)?;
    let scope = &session.scope;
    let license = visible(state, session, LICENSE, affectation.license).await?;
    let item = assets::get(
        &state.pool,
        affectation.kind,
        scope,
        affectation.item,
        false,
    )
    .await?;
    let license_writable = session.rights.has(LICENSE.right(), UPDATE)
        && scope.active().contains(&license.entities_id);
    let item_visible = item.is_some() && session.rights.has(affectation.kind.right(), READ);
    let item_writable = item
        .as_ref()
        .is_some_and(|item| scope.active().contains(&item.entities_id))
        && session.rights.has(affectation.kind.right(), UPDATE);
    // The license is visible: it was read in the scope.
    let allowed = (license_writable || item_writable) && (!license_writable || item_visible);
    let Some(item) = item.filter(|_| allowed) else {
        return Err(ApiError::Forbidden);
    };
    check_rules(
        state,
        session,
        affectation.license,
        affectation.kind,
        affectation.item,
        false,
    )
    .await?;
    let names = relation_names(state, session, &license, &item).await?;
    let actor = history::actor(session.user.id, &session.own_name());
    if !licenses::unaffect(&state.pool, id, affectation, &actor, &names).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A change of the validity of the license updates it like a form, which
/// runs the business rules of the assets on it (`assetBusinessRules`):
/// what they would change is left to the classic interface.
async fn check_rules(
    state: &AppState,
    session: &Session,
    license: i32,
    kind: AssetKind,
    item: i32,
    adding: bool,
) -> ApiResult<()> {
    let Some(valid) = licenses::validity_after(&state.pool, license, kind, item, adding).await?
    else {
        return Ok(());
    };
    let rules = nw_db::rules::asset_rules(&state.pool).await?;
    let input = business::FormInput::raw(&[
        ("id", license.to_string()),
        ("is_valid", i32::from(valid).to_string()),
    ]);
    let changes = business::run(
        &state.pool,
        session,
        &rules,
        Operation::Update,
        LICENSE,
        &input,
    )
    .await?;
    if changes.is_empty() {
        Ok(())
    } else {
        Err(ApiError::LegacyRequired("business_rules"))
    }
}

/// The license and the item as named in the history of each other, in the
/// language of the author.
async fn relation_names(
    state: &AppState,
    session: &Session,
    license: &assets::Detail,
    item: &assets::Detail,
) -> ApiResult<licenses::RelationNames> {
    let language = &session.preferences.language;
    let name_id = i18n::tr(language, text::NAME_ID);
    let dash = i18n::tr(language, text::DASH);
    let (name, software) = licenses::names_of(&state.pool, license.id)
        .await?
        .unwrap_or_default();
    Ok(licenses::RelationNames {
        license: version_history_name(
            &dash,
            &name_id,
            software.as_deref(),
            name.as_deref(),
            license.id,
        ),
        item: history::name_id(&name_id, item.name.as_deref(), item.id),
    })
}
