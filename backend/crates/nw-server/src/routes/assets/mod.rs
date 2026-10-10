//! Assets: computers, monitors, printers, network equipment, peripherals
//! and phones, under `/assets/{type}`.

pub mod choices;
mod guards;

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use nw_db::{
    assets::{self, ListQuery, Sort},
    events,
};
use nw_domain::{
    asset::{AssetKind, Field, FieldKind, Value},
    fk::fk,
    history,
    rights::{CREATE, DELETE, READ, UPDATE},
    text::{from_stored, to_stored},
};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    routes::auth::log_event,
    session::Session,
    state::AppState,
};

const DEFAULT_PER_PAGE: u32 = 20;
const MAX_PER_PAGE: u32 = 100;
/// Length of the VARCHAR(255) columns, in characters.
const MAX_TEXT_CHARS: usize = 255;
/// Size of the TEXT columns, in bytes.
const MAX_LONG_TEXT_BYTES: usize = 65_535;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/assets/{kind}", get(list).post(create))
        .route("/assets/{kind}/{id}", get(get_one).patch(update))
        .route("/assets/{kind}/{id}/trash", post(trash))
        .route("/assets/{kind}/{id}/restore", post(restore))
        .route("/assets/{kind}/choices/{field}", get(choices::choices))
}

/// Type of asset, in the paths.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AssetType {
    Computers,
    Monitors,
    Printers,
    NetworkEquipments,
    Peripherals,
    Phones,
}

impl From<AssetType> for AssetKind {
    fn from(kind: AssetType) -> Self {
        match kind {
            AssetType::Computers => Self::Computer,
            AssetType::Monitors => Self::Monitor,
            AssetType::Printers => Self::Printer,
            AssetType::NetworkEquipments => Self::NetworkEquipment,
            AssetType::Peripherals => Self::Peripheral,
            AssetType::Phones => Self::Phone,
        }
    }
}

impl From<AssetKind> for AssetType {
    fn from(kind: AssetKind) -> Self {
        match kind {
            AssetKind::Computer => Self::Computers,
            AssetKind::Monitor => Self::Monitors,
            AssetKind::Printer => Self::Printers,
            AssetKind::NetworkEquipment => Self::NetworkEquipments,
            AssetKind::Peripheral => Self::Peripherals,
            AssetKind::Phone => Self::Phones,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    Name,
    Serial,
    InventoryNumber,
    Entity,
    Location,
    Type,
    Manufacturer,
    State,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListParams {
    /// Page number, starting at 1.
    #[param(minimum = 1, default = 1)]
    pub page: Option<u32>,
    /// Items per page.
    #[param(minimum = 1, maximum = 100, default = 20)]
    pub per_page: Option<u32>,
    /// Text searched in the name, serial number, inventory number (and UUID
    /// of computers).
    pub search: Option<String>,
    /// List the items in the trash instead of the others.
    pub trash: Option<bool>,
    #[param(inline)]
    pub sort: Option<SortField>,
    #[param(inline)]
    pub order: Option<SortOrder>,
}

/// A related object: id and display name.
#[derive(Debug, Serialize, ToSchema)]
pub struct Ref {
    pub id: i32,
    /// `null` when the related row no longer exists.
    pub name: Option<String>,
}

impl Ref {
    fn optional(id: i32, name: Option<String>) -> Option<Self> {
        fk(id).map(|id| Self {
            id,
            name: decode(name),
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetSummary {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub inventory_number: Option<String>,
    pub entity: Ref,
    pub location: Option<Ref>,
    #[serde(rename = "type")]
    pub asset_type: Option<Ref>,
    pub manufacturer: Option<Ref>,
    pub state: Option<Ref>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetPage {
    pub items: Vec<AssetSummary>,
    pub page: u32,
    pub per_page: u32,
    /// Number of matching assets on every page.
    pub total: i64,
}

/// What the session may do with an asset.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetPermissions {
    pub update: bool,
    /// Move to the trash and restore.
    pub trash: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetDetail {
    pub kind: AssetType,
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub inventory_number: Option<String>,
    /// Computers only.
    pub uuid: Option<String>,
    /// Peripherals and phones only.
    pub brand: Option<String>,
    /// Phones only.
    pub number_line: Option<String>,
    pub contact: Option<String>,
    pub contact_number: Option<String>,
    pub comment: Option<String>,
    /// In the trash.
    pub is_deleted: bool,
    /// Updated by an inventory tool.
    pub is_dynamic: bool,
    /// Visible in sub-entities.
    pub is_recursive: bool,
    /// Used by several computers at once.
    pub is_global: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub entity: Ref,
    pub location: Option<Ref>,
    #[serde(rename = "type")]
    pub asset_type: Option<Ref>,
    pub model: Option<Ref>,
    pub manufacturer: Option<Ref>,
    pub state: Option<Ref>,
    /// Computers, printers and network equipment only.
    pub network: Option<Ref>,
    /// Computers only: how the computer is updated (inventory tool...).
    pub update_source: Option<Ref>,
    pub user: Option<Ref>,
    pub group: Option<Ref>,
    pub tech_user: Option<Ref>,
    pub tech_group: Option<Ref>,
    pub permissions: AssetPermissions,
}

/// Fields of a creation or an update. Absent fields are left unchanged;
/// `null` clears a field (empty text, no related object).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct AssetFields {
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub serial: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub inventory_number: Option<String>,
    /// Computers only.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub uuid: Option<String>,
    /// Peripherals and phones only.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub brand: Option<String>,
    /// Phones only.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub number_line: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub contact: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub contact_number: Option<String>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub comment: Option<String>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub location_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub state_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub manufacturer_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub type_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub model_id: Option<i32>,
    /// Computers, printers and network equipment only.
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub network_id: Option<i32>,
    /// Computers only.
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub update_source_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub user_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub group_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub tech_user_id: Option<i32>,
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub tech_group_id: Option<i32>,
    /// Visible in sub-entities. Can be set, not unset (classic interface).
    pub is_recursive: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssetCreation {
    /// Entity of the new asset: one of the selected entities.
    pub entity_id: i32,
    #[serde(flatten)]
    pub fields: AssetFields,
}

fn nullable_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer).map(|text| Some(text.unwrap_or_default()))
}

fn nullable_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i32>, D::Error> {
    Option::<i32>::deserialize(deserializer).map(|id| Some(id.unwrap_or(0)))
}

/// Lists the assets of a type visible to the session (templates excluded).
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}",
    tag = "assets",
    params(("kind" = AssetType, Path, description = "Type of asset"), ListParams),
    responses(
        (status = 200, description = "One page of assets", body = AssetPage),
        (status = 400, description = "Invalid parameters", body = ErrorBody),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on this type", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    kind: Result<Path<AssetType>, PathRejection>,
    params: Result<Query<ListParams>, QueryRejection>,
) -> ApiResult<Json<AssetPage>> {
    let Path(kind) = kind.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    session.require(kind.right(), READ)?;
    let Query(params) = params.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let query = ListQuery {
        search: params
            .search
            .map(|search| search.trim().to_owned())
            .filter(|search| !search.is_empty()),
        trash: params.trash.unwrap_or(false),
        sort: params.sort.unwrap_or_default().into(),
        descending: matches!(params.order, Some(SortOrder::Desc)),
        limit: per_page,
        offset: (page - 1).saturating_mul(per_page),
    };
    let (rows, total) = assets::list(&state.pool, kind, &session.scope, &query).await?;

    Ok(Json(AssetPage {
        items: rows.into_iter().map(AssetSummary::from).collect(),
        page,
        per_page,
        total,
    }))
}

/// Returns an asset visible to the session, including one in the trash.
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 200, description = "The asset", body = AssetDetail),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on this type", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
    )
)]
pub async fn get_one(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<Json<AssetDetail>> {
    let (kind, id) = parse_path(path)?;
    session.require(kind.right(), READ)?;
    Ok(Json(detail(&state, &session, kind, id).await?))
}

/// Creates an asset in one of the selected entities.
#[utoipa::path(
    post,
    path = "/api/v2/assets/{kind}",
    tag = "assets",
    params(("kind" = AssetType, Path, description = "Type of asset")),
    request_body = AssetCreation,
    responses(
        (status = 201, description = "Created", body = AssetDetail),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No create right, or entity not selected", body = ErrorBody),
        (status = 409, description = "Must be created in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    kind: Result<Path<AssetType>, PathRejection>,
    payload: Result<Json<AssetCreation>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<AssetDetail>)> {
    let Path(kind) = kind.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    session.require(kind.right(), CREATE)?;
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    // Legacy CommonDBTM::canCreateItem: an entity of the session, not a parent.
    if !session.scope.active().contains(&request.entity_id) {
        return Err(ApiError::Forbidden);
    }
    let values = field_values(kind, &request.fields)?;
    check_references(&state, &values).await?;
    guards::check_create(&state.pool, &session.tree, kind, request.entity_id, &values).await?;

    let actor = history::actor(session.user.id, &session.own_name());
    let id = assets::create(&state.pool, kind, request.entity_id, &values, &actor).await?;
    let name = request
        .fields
        .name
        .as_deref()
        .map(to_stored)
        .unwrap_or_default();
    log_asset_event(&state, &session, kind, id, &format!("adds the item {name}")).await;
    Ok((
        StatusCode::CREATED,
        Json(detail(&state, &session, kind, id).await?),
    ))
}

/// Updates an asset of a selected entity. For computers, the changes of
/// contact, user, group, location (and status, depending on the settings)
/// are copied to the connected items and components, like the PHP code.
#[utoipa::path(
    patch,
    path = "/api/v2/assets/{kind}/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    request_body = AssetFields,
    responses(
        (status = 200, description = "Updated", body = AssetDetail),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No update right, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Must be changed in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    payload: Result<Json<AssetFields>, JsonRejection>,
) -> ApiResult<Json<AssetDetail>> {
    let (kind, id) = parse_path(path)?;
    session.require(kind.right(), UPDATE)?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let current = writable(&state, &session, kind, id).await?;
    let values = field_values(kind, &fields)?;
    check_references(&state, &values).await?;
    let propagation = guards::propagation(&state.pool).await?;
    guards::check_update(
        &state.pool,
        &session.tree,
        kind,
        &current,
        &values,
        propagation,
    )
    .await?;

    let actor = history::actor(session.user.id, &session.own_name());
    assets::update(&state.pool, kind, id, &values, &actor, propagation)
        .await?
        .ok_or(ApiError::NotFound)?;
    log_asset_event(&state, &session, kind, id, "updates an item").await;
    Ok(Json(detail(&state, &session, kind, id).await?))
}

/// Moves an asset to the trash.
#[utoipa::path(
    post,
    path = "/api/v2/assets/{kind}/{id}/trash",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 200, description = "In the trash", body = AssetDetail),
        (status = 403, description = "No delete right, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Must be done in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn trash(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<Json<AssetDetail>> {
    set_trashed(state, session, path, true).await
}

/// Restores an asset from the trash.
#[utoipa::path(
    post,
    path = "/api/v2/assets/{kind}/{id}/restore",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 200, description = "Restored", body = AssetDetail),
        (status = 403, description = "No delete right, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Must be done in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn restore(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<Json<AssetDetail>> {
    set_trashed(state, session, path, false).await
}

async fn set_trashed(
    state: AppState,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    trashed: bool,
) -> ApiResult<Json<AssetDetail>> {
    let (kind, id) = parse_path(path)?;
    // Legacy forms check DELETE for both actions.
    session.require(kind.right(), DELETE)?;
    let current = writable(&state, &session, kind, id).await?;
    if current.is_deleted != trashed {
        guards::check_trash(&state.pool).await?;
    }
    let actor = history::actor(session.user.id, &session.own_name());
    let changed = assets::set_trashed(&state.pool, kind, id, trashed, &actor)
        .await?
        .ok_or(ApiError::NotFound)?;
    if changed {
        let action = if trashed {
            "deletes an item"
        } else {
            "restores an item"
        };
        log_asset_event(&state, &session, kind, id, action).await;
    }
    Ok(Json(detail(&state, &session, kind, id).await?))
}

fn parse_path(path: Result<Path<(AssetType, i32)>, PathRejection>) -> ApiResult<(AssetKind, i32)> {
    let Path((kind, id)) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    Ok((kind.into(), id))
}

/// The asset, if visible, and in an entity of the session (legacy
/// `canUpdateItem`: items shared from a parent entity are read-only).
async fn writable(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
) -> ApiResult<assets::Current> {
    let visible = assets::get(&state.pool, kind, &session.scope, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !session.scope.active().contains(&visible.entities_id) {
        return Err(ApiError::Forbidden);
    }
    assets::current(&state.pool, kind, id)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn detail(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
) -> ApiResult<AssetDetail> {
    let row = assets::get(&state.pool, kind, &session.scope, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let in_selected_entity = session.scope.active().contains(&row.entities_id);
    let permissions = AssetPermissions {
        update: in_selected_entity && session.rights.has(kind.right(), UPDATE),
        trash: in_selected_entity && session.rights.has(kind.right(), DELETE),
    };
    Ok(AssetDetail::from_row(kind, row, session, permissions))
}

/// Stored values of the given fields, validated.
fn field_values(kind: AssetKind, fields: &AssetFields) -> ApiResult<Vec<(Field, Value)>> {
    let (type_column, _) = kind.type_dropdown();
    let (model_column, _) = kind.model_dropdown();
    let texts = [
        ("name", "name", &fields.name),
        ("serial", "serial", &fields.serial),
        ("inventory_number", "otherserial", &fields.inventory_number),
        ("uuid", "uuid", &fields.uuid),
        ("brand", "brand", &fields.brand),
        ("number_line", "number_line", &fields.number_line),
        ("contact", "contact", &fields.contact),
        ("contact_number", "contact_num", &fields.contact_number),
        ("comment", "comment", &fields.comment),
    ];
    let ids = [
        ("location_id", "locations_id", fields.location_id),
        ("state_id", "states_id", fields.state_id),
        (
            "manufacturer_id",
            "manufacturers_id",
            fields.manufacturer_id,
        ),
        ("type_id", type_column, fields.type_id),
        ("model_id", model_column, fields.model_id),
        ("network_id", "networks_id", fields.network_id),
        (
            "update_source_id",
            "autoupdatesystems_id",
            fields.update_source_id,
        ),
        ("user_id", "users_id", fields.user_id),
        ("group_id", "groups_id", fields.group_id),
        ("tech_user_id", "users_id_tech", fields.tech_user_id),
        ("tech_group_id", "groups_id_tech", fields.tech_group_id),
    ];
    let field = |name: &str, column: &str| {
        kind.field(column).ok_or_else(|| {
            ApiError::BadRequest(format!("{name}: no such field for {}", kind.slug()))
        })
    };

    let mut values = Vec::new();
    for (name, column, text) in texts {
        let Some(text) = text else { continue };
        let stored = to_stored(text);
        let too_long = if column == "comment" {
            stored.len() > MAX_LONG_TEXT_BYTES
        } else {
            stored.chars().count() > MAX_TEXT_CHARS
        };
        if too_long {
            return Err(ApiError::BadRequest(format!("{name}: too long")));
        }
        values.push((field(name, column)?, Value::Text(Some(stored))));
    }
    for (name, column, id) in ids {
        let Some(id) = id else { continue };
        if id < 0 {
            return Err(ApiError::BadRequest(format!("{name}: invalid id")));
        }
        values.push((field(name, column)?, Value::Id(id)));
    }
    if let Some(recursive) = fields.is_recursive {
        values.push((
            field("is_recursive", "is_recursive")?,
            Value::Bool(recursive),
        ));
    }
    Ok(values)
}

async fn check_references(state: &AppState, values: &[(Field, Value)]) -> ApiResult<()> {
    let missing = assets::references_exist(&state.pool, values).await?;
    match missing.first() {
        Some(column) => Err(ApiError::BadRequest(format!("{column}: unknown id"))),
        None => Ok(()),
    }
}

/// Event of the legacy asset forms: `<login> <action>`.
async fn log_asset_event(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    action: &str,
) {
    let login = session.user.name.clone().unwrap_or_default();
    log_event(
        state,
        &events::Event {
            items_id: id,
            kind: kind.event_type(),
            service: "inventory",
            level: 4,
            message: &format!("{login} {action}"),
        },
    )
    .await;
}

fn decode(text: Option<String>) -> Option<String> {
    text.map(|text| from_stored(&text))
}

impl From<SortField> for Sort {
    fn from(field: SortField) -> Self {
        match field {
            SortField::Name => Self::Name,
            SortField::Serial => Self::Serial,
            SortField::InventoryNumber => Self::InventoryNumber,
            SortField::Entity => Self::Entity,
            SortField::Location => Self::Location,
            SortField::Type => Self::Type,
            SortField::Manufacturer => Self::Manufacturer,
            SortField::State => Self::State,
            SortField::UpdatedAt => Self::UpdatedAt,
        }
    }
}

impl From<assets::ListItem> for AssetSummary {
    fn from(row: assets::ListItem) -> Self {
        Self {
            id: row.id,
            name: decode(row.name),
            serial: decode(row.serial),
            inventory_number: decode(row.otherserial),
            entity: Ref {
                id: row.entities_id,
                name: decode(row.entity),
            },
            location: Ref::optional(row.locations_id, row.location),
            asset_type: Ref::optional(row.types_id, row.type_name),
            manufacturer: Ref::optional(row.manufacturers_id, row.manufacturer),
            state: Ref::optional(row.states_id, row.state),
            updated_at: row.date_mod,
        }
    }
}

impl AssetDetail {
    fn from_row(
        kind: AssetKind,
        row: assets::Detail,
        session: &Session,
        permissions: AssetPermissions,
    ) -> Self {
        // `None` when the user row no longer exists.
        let user_name = |id: i32,
                         login: Option<String>,
                         realname: Option<String>,
                         firstname: Option<String>| {
            login.map(|login| {
                session.user_name(id, Some(&login), realname.as_deref(), firstname.as_deref())
            })
        };
        let has = |column: &str| {
            kind.field(column)
                .is_some_and(|field| field.kind == FieldKind::Text)
        };
        Self {
            kind: kind.into(),
            id: row.id,
            name: decode(row.name),
            serial: decode(row.serial),
            inventory_number: decode(row.otherserial),
            uuid: if has("uuid") { decode(row.uuid) } else { None },
            brand: if has("brand") {
                decode(row.brand)
            } else {
                None
            },
            number_line: if has("number_line") {
                decode(row.number_line)
            } else {
                None
            },
            contact: decode(row.contact),
            contact_number: decode(row.contact_num),
            comment: decode(row.comment),
            is_deleted: row.is_deleted,
            is_dynamic: row.is_dynamic,
            is_recursive: row.is_recursive,
            is_global: row.is_global != 0,
            created_at: row.date_creation,
            updated_at: row.date_mod,
            entity: Ref {
                id: row.entities_id,
                name: decode(row.entity),
            },
            location: Ref::optional(row.locations_id, row.location),
            asset_type: Ref::optional(row.types_id, row.type_name),
            model: Ref::optional(row.models_id, row.model_name),
            manufacturer: Ref::optional(row.manufacturers_id, row.manufacturer),
            state: Ref::optional(row.states_id, row.state),
            network: Ref::optional(row.networks_id, row.network),
            update_source: Ref::optional(row.autoupdatesystems_id, row.autoupdatesystem),
            user: Ref::optional(
                row.users_id,
                user_name(
                    row.users_id,
                    row.user_name,
                    row.user_realname,
                    row.user_firstname,
                ),
            ),
            group: Ref::optional(row.groups_id, row.group),
            tech_user: Ref::optional(
                row.users_id_tech,
                user_name(
                    row.users_id_tech,
                    row.tech_name,
                    row.tech_realname,
                    row.tech_firstname,
                ),
            ),
            tech_group: Ref::optional(row.groups_id_tech, row.tech_group),
            permissions,
        }
    }
}
