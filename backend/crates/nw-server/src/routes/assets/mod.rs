//! Assets: computers, monitors, printers, network equipment, peripherals
//! and phones, and the datacenter items (racks, enclosures, PDUs, passive
//! equipment, clusters), under `/assets/{type}`.

mod business;
pub mod choices;
mod guards;
pub mod racks;
pub mod templates;
pub mod unicity;

use std::collections::{HashMap, HashSet, hash_map::Entry};

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
    events, infocoms,
};
use nw_domain::{
    asset::{AssetKind, Field, FieldKind, Value, filter_decimal, filter_integer},
    fk::fk,
    history,
    i18n::{self, Source, text},
    rack,
    rights::{CREATE, DELETE, PURGE, READ, UPDATE},
    rules::Operation,
    template::ClonedLink,
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
        .route(
            "/assets/{kind}/{id}",
            get(get_one).patch(update).delete(purge),
        )
        .route(
            "/assets/{kind}/templates",
            get(templates::list).post(templates::create),
        )
        .route(
            "/assets/{kind}/templates/{id}",
            get(templates::get_one)
                .patch(templates::update)
                .delete(templates::purge),
        )
        .route(
            "/assets/{kind}/templates/{id}/items",
            post(templates::create_item),
        )
        .route("/assets/{kind}/{id}/trash", post(trash))
        .route("/assets/{kind}/{id}/restore", post(restore))
        .route("/assets/{kind}/choices/{field}", get(choices::choices))
        .route("/assets/racks/positions", get(racks::positions))
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
    Racks,
    Enclosures,
    Pdus,
    PassiveDcEquipments,
    Clusters,
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
            AssetType::Racks => Self::Rack,
            AssetType::Enclosures => Self::Enclosure,
            AssetType::Pdus => Self::Pdu,
            AssetType::PassiveDcEquipments => Self::PassiveDcEquipment,
            AssetType::Clusters => Self::Cluster,
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
            AssetKind::Rack => Self::Racks,
            AssetKind::Enclosure => Self::Enclosures,
            AssetKind::Pdu => Self::Pdus,
            AssetKind::PassiveDcEquipment => Self::PassiveDcEquipments,
            AssetKind::Cluster => Self::Clusters,
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
#[derive(Debug, Clone, Serialize, ToSchema)]
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
    /// Delete for good, from the trash.
    pub purge: bool,
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
    /// Monitors only: diagonal in inches, as stored (`"21.50"`).
    pub size: Option<String>,
    /// Printers (free text) and network equipment (MiB) only.
    pub memory: Option<String>,
    /// Printers only.
    pub initial_page_counter: Option<i32>,
    /// Printers only: current counter of pages.
    pub page_counter: Option<i32>,
    /// Phones only.
    pub power_supply: Option<Ref>,
    /// Connectors and features of the type, `null` for the other types.
    #[serde(flatten)]
    pub features: Features,
    /// Fields of the datacenter items, `null` for the other types.
    #[serde(flatten)]
    pub datacenter: DatacenterDetail,
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
    /// A template: a model of the items of its type.
    pub is_template: bool,
    /// Name of a template.
    pub template_name: Option<String>,
    pub permissions: AssetPermissions,
    /// After a write: items with the same values as this one, reported by
    /// a unicity criterion that does not refuse them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub duplicates: Vec<unicity::DuplicateNotice>,
}

/// Fields of the datacenter items.
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct DatacenterDetail {
    /// Racks: server room.
    pub room: Option<Ref>,
    /// Racks: place in the room, `"<column>,<row>"` (`"0"` or empty for
    /// none).
    pub position: Option<String>,
    /// Racks: door orientation in the room, 1 north, 2 east, 3 south, 4 west
    /// (0: never set).
    pub room_orientation: Option<i32>,
    /// Racks.
    pub number_units: Option<i32>,
    /// Racks.
    pub width: Option<i32>,
    /// Racks.
    pub height: Option<i32>,
    /// Racks.
    pub depth: Option<i32>,
    /// Racks: in watts.
    pub max_power: Option<i32>,
    /// Racks: in watts.
    pub measured_power: Option<i32>,
    /// Racks.
    pub max_weight: Option<i32>,
    /// Racks: `#RRGGBB`.
    pub background_color: Option<String>,
    /// Enclosures.
    pub power_supplies: Option<i32>,
    /// Clusters.
    pub version: Option<String>,
}

impl DatacenterDetail {
    fn read(specific: &assets::Specific) -> Self {
        let number = |column: &str| specific.text(column).and_then(|value| value.parse().ok());
        Self {
            room: number("dcrooms_id").and_then(|id| Ref::optional(id, specific.room.clone())),
            position: decode(specific.text("position")),
            room_orientation: number("room_orientation"),
            number_units: number("number_units"),
            width: number("width"),
            height: number("height"),
            depth: number("depth"),
            max_power: number("max_power"),
            measured_power: number("mesured_power"),
            max_weight: number("max_weight"),
            background_color: specific.text("bgcolor"),
            power_supplies: number("power_supplies"),
            version: decode(specific.text("version")),
        }
    }
}

/// Fields of the datacenter items in a creation or an update.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct DatacenterFields {
    /// Racks: server room.
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub room_id: Option<i32>,
    /// Racks: place in the room, `"<column>,<row>"` as offered by
    /// `/assets/racks/positions`, `"0"` for none.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub position: Option<String>,
    /// Racks: door orientation, 1 north, 2 east, 3 south, 4 west.
    pub room_orientation: Option<i32>,
    /// Racks: 1 to 100.
    pub number_units: Option<i32>,
    /// Racks; `null` empties it (stored as 0, like the classic form).
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub width: Option<Option<i32>>,
    /// Racks; `null` empties it.
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub height: Option<Option<i32>>,
    /// Racks; `null` empties it.
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub depth: Option<Option<i32>>,
    /// Racks: in watts; `null` empties it.
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub max_power: Option<Option<i32>>,
    /// Racks: in watts; `null` empties it.
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub measured_power: Option<Option<i32>>,
    /// Racks; `null` empties it.
    #[serde(default, deserialize_with = "nullable_number")]
    #[schema(value_type = Option<i32>)]
    pub max_weight: Option<Option<i32>>,
    /// Racks: `#RRGGBB`.
    pub background_color: Option<String>,
    /// Enclosures.
    pub power_supplies: Option<i32>,
    /// Clusters.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub version: Option<String>,
}

/// Connectors and features of monitors, printers and phones (`have_*`).
#[derive(Debug, Default, Deserialize, Serialize, ToSchema)]
pub struct Features {
    /// Monitors.
    pub has_microphone: Option<bool>,
    /// Monitors and phones.
    pub has_speaker: Option<bool>,
    /// Monitors.
    pub has_subd: Option<bool>,
    /// Monitors.
    pub has_bnc: Option<bool>,
    /// Monitors.
    pub has_dvi: Option<bool>,
    /// Monitors.
    pub has_pivot: Option<bool>,
    /// Monitors.
    pub has_hdmi: Option<bool>,
    /// Monitors.
    pub has_displayport: Option<bool>,
    /// Printers: serial port.
    pub has_serial: Option<bool>,
    /// Printers.
    pub has_parallel: Option<bool>,
    /// Printers.
    pub has_usb: Option<bool>,
    /// Printers.
    pub has_ethernet: Option<bool>,
    /// Printers.
    pub has_wifi: Option<bool>,
    /// Phones.
    pub has_headset: Option<bool>,
}

impl Features {
    /// `(API name, value)` with the column of the type.
    fn columns(&self, kind: AssetKind) -> [(&'static str, &'static str, Option<bool>); 14] {
        let speaker = if kind == AssetKind::Phone {
            "have_hp"
        } else {
            "have_speaker"
        };
        [
            ("has_microphone", "have_micro", self.has_microphone),
            ("has_speaker", speaker, self.has_speaker),
            ("has_subd", "have_subd", self.has_subd),
            ("has_bnc", "have_bnc", self.has_bnc),
            ("has_dvi", "have_dvi", self.has_dvi),
            ("has_pivot", "have_pivot", self.has_pivot),
            ("has_hdmi", "have_hdmi", self.has_hdmi),
            ("has_displayport", "have_displayport", self.has_displayport),
            ("has_serial", "have_serial", self.has_serial),
            ("has_parallel", "have_parallel", self.has_parallel),
            ("has_usb", "have_usb", self.has_usb),
            ("has_ethernet", "have_ethernet", self.has_ethernet),
            ("has_wifi", "have_wifi", self.has_wifi),
            ("has_headset", "have_headset", self.has_headset),
        ]
    }

    fn read(kind: AssetKind, specific: &assets::Specific) -> Self {
        let flag = |column: &str| specific.flag(column);
        Self {
            has_microphone: flag("have_micro"),
            has_speaker: flag(if kind == AssetKind::Phone {
                "have_hp"
            } else {
                "have_speaker"
            }),
            has_subd: flag("have_subd"),
            has_bnc: flag("have_bnc"),
            has_dvi: flag("have_dvi"),
            has_pivot: flag("have_pivot"),
            has_hdmi: flag("have_hdmi"),
            has_displayport: flag("have_displayport"),
            has_serial: flag("have_serial"),
            has_parallel: flag("have_parallel"),
            has_usb: flag("have_usb"),
            has_ethernet: flag("have_ethernet"),
            has_wifi: flag("have_wifi"),
            has_headset: flag("have_headset"),
        }
    }
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
    /// Monitors only: decimal number of inches (`"21.5"`, `"21,5"`).
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub size: Option<String>,
    /// Printers: free text. Network equipment: a whole number of MiB, or
    /// empty.
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub memory: Option<String>,
    /// Printers only.
    pub initial_page_counter: Option<i32>,
    /// Printers only.
    pub page_counter: Option<i32>,
    /// Phones only.
    #[serde(default, deserialize_with = "nullable_id")]
    #[schema(value_type = Option<i32>)]
    pub power_supply_id: Option<i32>,
    /// Monitors, printers, peripherals and phones only: one item for
    /// several computers.
    pub is_global: Option<bool>,
    #[serde(flatten)]
    pub features: Features,
    #[serde(flatten)]
    pub datacenter: DatacenterFields,
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

/// A number that can be emptied: absent, `null` or a number.
fn nullable_number<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<i32>>, D::Error> {
    Option::<i32>::deserialize(deserializer).map(Some)
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
    Ok(Json(detail(&state, &session, kind, id, false).await?))
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
        (status = 409, description = "Another item has the same values (`duplicate`), or must be created in the classic interface (`legacy_required`)", body = ErrorBody),
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
    let requested = field_values(kind, &request.fields)?;
    let created = create_asset(
        &state,
        &session,
        kind,
        request.entity_id,
        requested,
        Creation::Item,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// What a creation makes.
enum Creation {
    /// An item, from the form of a new item.
    Item,
    /// A template, from the form of a new template.
    Template,
    /// An item from the form of this template: its stored values fill the
    /// fields not given, as the form shows them.
    FromTemplate(assets::Current),
}

/// Creates an asset like the legacy forms (`CommonDBTM::add`, `clone` from
/// a template): the rules of a creation, the unicity of the fields (not
/// for a template), the event. Returns the new asset.
async fn create_asset(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    entity_id: i32,
    requested: Vec<(Field, Value)>,
    creation: Creation,
) -> ApiResult<AssetDetail> {
    // Legacy CommonDBTM::canCreateItem: an entity of the session, not a parent.
    if !session.scope.active().contains(&entity_id) {
        return Err(ApiError::Forbidden);
    }
    check_references(state, &requested).await?;
    let infocom_settings = infocoms::settings(&state.pool).await?;
    // Infocom written: created with the item (not for a clone), dated by
    // its status (not for a template), cloned from the template.
    let (writes_infocom, clones_infocom) = match &creation {
        _ if !kind.has_infocom() => (false, false),
        Creation::Item => (
            infocom_settings.auto_create || infocom_settings.any_status_date(),
            false,
        ),
        Creation::Template => (infocom_settings.auto_create, false),
        // Only the inventory assets clone the infocom of their template.
        Creation::FromTemplate(template) => (
            infocom_settings.any_status_date(),
            kind.cloned_links().contains(&ClonedLink::Infocom)
                && infocoms::exists(&state.pool, kind.itemtype(), template.id).await?,
        ),
    };
    guards::check_create(&state.pool, writes_infocom, clones_infocom).await?;
    if let Creation::FromTemplate(template) = &creation {
        guards::check_template(&state.pool, kind, template.id).await?;
    }

    // Every field of the form is written as the classic interface posts
    // it: the given value, else the value of a new item (an empty text,
    // not NULL) or of the template as the form shows it. The rules see
    // them all.
    let fields = match creation {
        Creation::Template => kind.template_fields(),
        Creation::Item | Creation::FromTemplate(_) => kind.fields(),
    };
    let mut values: Vec<(Field, Value)> = fields
        .into_iter()
        .filter_map(|field| {
            let posted = requested
                .iter()
                .find(|(given, _)| given.column == field.column);
            let value = match (posted, &creation) {
                (Some((_, value)), _) => value.clone(),
                (None, Creation::FromTemplate(template)) => {
                    kind.form_value(field, template.value(field.column)?)
                }
                (None, Creation::Item | Creation::Template) => kind.form_default(field)?,
            };
            Some((field, value))
        })
        .collect();
    // The event names the item as posted.
    let posted_name = values
        .iter()
        .find_map(|(field, value)| match value {
            Value::Text(name) if field.column == "name" => name.clone(),
            _ => None,
        })
        .unwrap_or_default();
    // `prepareInputForAdd` of a rack, before the rules and the unicity.
    check_rack_position(state, kind, &values, None).await?;
    // Only the inventory assets have business rules.
    if kind.is_inventory_asset() {
        let rules = nw_db::rules::asset_rules(&state.pool).await?;
        let rule_values = business::run(
            &state.pool,
            session,
            &rules,
            Operation::Add,
            kind,
            &business::FormInput::form(&values, entity_id, None),
        )
        .await?;
        business::merge(&mut values, rule_values);
    }

    // Unicity of the values the rules leave; not for a template.
    let posted = business::FormInput::form(&values, entity_id, None).by_field();
    let checked = match &creation {
        Creation::Template => None,
        _ if !kind.has_unicity() => None,
        Creation::Item => Some(
            unicity::check(
                state,
                session,
                kind,
                entity_id,
                unicity::Posted::Form(&posted),
                None,
            )
            .await?,
        ),
        Creation::FromTemplate(template) => {
            // The clone posts every column of the template.
            let mut input = assets::row_texts(&state.pool, kind, template.id).await?;
            for column in [
                "id",
                "date_creation",
                "date_mod",
                "is_template",
                "template_name",
            ] {
                input.remove(column);
            }
            input.extend(posted);
            Some(
                unicity::check(
                    state,
                    session,
                    kind,
                    entity_id,
                    unicity::Posted::Fields(&input),
                    None,
                )
                .await?,
            )
        }
    };
    let notices = match checked {
        Some(checked) => checked.log_and_enforce(state, kind, None).await?,
        None => Vec::new(),
    };

    let actor = history::actor(session.user.id, &session.own_name());
    let name_id = i18n::tr(&session.preferences.language, text::NAME_ID);
    let origin = match &creation {
        Creation::Item => assets::Origin::Form,
        Creation::Template => assets::Origin::Template,
        Creation::FromTemplate(template) => assets::Origin::FromTemplate {
            id: template.id,
            name_id: &name_id,
        },
    };
    let id = assets::create(
        &state.pool,
        kind,
        entity_id,
        &values,
        &actor,
        &infocom_settings,
        origin,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    log_asset_event(state, session, kind, id, text::ADDS_ITEM, &[&posted_name]).await;
    let template = matches!(creation, Creation::Template);
    let mut created = detail(state, session, kind, id, template).await?;
    created.duplicates = notices;
    Ok(created)
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
        (status = 409, description = "Another item has the same values (`duplicate`), items of other entities use the shared item (`sharing_in_use`), or must be changed in the classic interface (`legacy_required`)", body = ErrorBody),
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
    let values = field_values(kind, &fields)?;
    Ok(Json(
        update_asset(&state, &session, kind, id, values, false).await?,
    ))
}

/// Updates an asset (or a template) like the legacy forms: the rules of an
/// update, the unicity of the fields (not for a template), the changes of
/// a computer copied to its connected items, the event.
async fn update_asset(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    mut values: Vec<(Field, Value)>,
    template: bool,
) -> ApiResult<AssetDetail> {
    let current = writable(state, session, kind, id, template).await?;
    check_references(state, &values).await?;

    // The legacy form posts the whole form: the stored values and the
    // changes. `prepareInputForUpdate` of a rack checks its place first; a
    // refused update still logs the event of the form.
    let mut form = current.values.clone();
    business::merge(&mut form, values.clone());
    if let Err(error) = check_rack_position(state, kind, &form, Some(id)).await {
        log_asset_event(state, session, kind, id, text::UPDATES_ITEM, &[]).await;
        return Err(error);
    }
    // The rules see the whole form; only the inventory assets have some.
    let rules = if kind.is_inventory_asset() {
        nw_db::rules::asset_rules(&state.pool).await?
    } else {
        Vec::new()
    };
    if kind.is_inventory_asset() {
        let rule_values = business::run(
            &state.pool,
            session,
            &rules,
            Operation::Update,
            kind,
            &business::FormInput::form(&form, current.entities_id, Some(id)),
        )
        .await?;
        business::merge(&mut values, rule_values);
    }
    // The changes, and their history, in the order of the legacy form.
    kind.sort_by_form(&mut values, template);

    // Unicity of the whole form, checked even when nothing changes; not
    // for a template.
    business::merge(&mut form, values.clone());
    let posted = business::FormInput::form(&form, current.entities_id, Some(id)).by_field();
    let checked = if template || !kind.has_unicity() {
        None
    } else {
        Some(
            unicity::check(
                state,
                session,
                kind,
                current.entities_id,
                unicity::Posted::Form(&posted),
                Some(id),
            )
            .await?,
        )
    };

    // A computer copies some changes to its connected items, whose own
    // update runs the rules, then the unicity check, on the copied fields.
    let changed = guards::changed_columns(&current, &values);
    let propagation = guards::propagation(&state.pool).await?;
    let copied: Vec<(Field, Value)> = propagation
        .to_connected_items(&changed)
        .into_iter()
        .filter_map(|column| {
            values
                .iter()
                .find(|(field, _)| field.column == column)
                .cloned()
        })
        .collect();
    let mut connected_rules: HashMap<AssetKind, Vec<(Field, Value)>> = HashMap::new();
    let mut connected_checks = Vec::new();
    if kind == AssetKind::Computer && !copied.is_empty() {
        for item in assets::connected(&state.pool, id).await? {
            let mut item_values: Vec<(Field, Value)> = copied
                .iter()
                .filter_map(|(field, value)| Some((item.kind.field(field.column)?, value.clone())))
                .collect();
            if let Entry::Vacant(entry) = connected_rules.entry(item.kind) {
                let input = business::FormInput::fields(&item_values);
                let rule_values = business::run(
                    &state.pool,
                    session,
                    &rules,
                    Operation::Update,
                    item.kind,
                    &input,
                )
                .await?;
                entry.insert(rule_values);
            }
            business::merge(&mut item_values, connected_rules[&item.kind].clone());
            let mut posted = business::FormInput::fields(&item_values).by_field();
            posted.insert("id".to_owned(), item.id.to_string());
            let checked = unicity::check(
                state,
                session,
                item.kind,
                item.entities_id,
                unicity::Posted::Fields(&posted),
                Some(item.id),
            )
            .await?;
            connected_checks.push((item, checked));
        }
    }
    let infocom_settings = infocoms::settings(&state.pool).await?;
    guards::check_update(
        &state.pool,
        &session.tree,
        kind,
        &current,
        &changed,
        propagation,
        &infocom_settings,
    )
    .await?;

    // Events of the duplicates, as the PHP code logs them on the way.
    let notices = match checked {
        Some(checked) => match checked.log_and_enforce(state, kind, Some(id)).await {
            Ok(notices) => notices,
            Err(error) => {
                // The legacy form logs its event even when the update is
                // refused.
                log_asset_event(state, session, kind, id, text::UPDATES_ITEM, &[]).await;
                return Err(error);
            }
        },
        None => Vec::new(),
    };
    let mut refused_items = HashSet::new();
    for (item, checked) in &connected_checks {
        if checked.log(state, item.kind, Some(item.id)).await {
            refused_items.insert((item.kind, item.id));
        }
    }

    let actor = history::actor(session.user.id, &session.own_name());
    let effects = assets::UpdateEffects {
        propagation,
        connected_rules: &connected_rules,
        refused_items: &refused_items,
        infocoms: &infocom_settings,
    };
    assets::update(&state.pool, kind, id, &values, &actor, effects)
        .await?
        .ok_or(ApiError::NotFound)?;
    log_asset_event(state, session, kind, id, text::UPDATES_ITEM, &[]).await;
    let mut updated = detail(state, session, kind, id, template).await?;
    updated.duplicates = notices;
    Ok(updated)
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

/// Deletes an asset of the trash for good, with its history and the rows
/// the PHP code deletes with it (contracts, documents, connections,
/// infocom...). Linked data whose purge is not ported (network ports,
/// components, software, tickets...) leaves it to the classic interface.
#[utoipa::path(
    delete,
    path = "/api/v2/assets/{kind}/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Asset id"),
    ),
    responses(
        (status = 204, description = "Deleted for good"),
        (status = 403, description = "No purge right (on the type, or on its infocom), or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Not in the trash (`not_in_trash`), or must be done in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn purge(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let (kind, id) = parse_path(path)?;
    purge_asset(&state, &session, kind, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Purges an asset of the trash, or a template.
async fn purge_asset(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    template: bool,
) -> ApiResult<()> {
    session.require(kind.right(), PURGE)?;
    let current = writable(state, session, kind, id, template).await?;
    // The classic interface offers it in the trash only; a template, in
    // the list of the templates.
    if !template && !current.is_deleted {
        return Err(ApiError::NotInTrash);
    }
    // `canPurgeItem`: purging the infocom too.
    if infocoms::exists(&state.pool, kind.itemtype(), id).await?
        && !session.rights.has("infocom", PURGE)
    {
        return Err(ApiError::Forbidden);
    }
    guards::check_purge(&state.pool, kind, id).await?;
    if !assets::purge(&state.pool, kind, id).await? {
        return Err(ApiError::NotFound);
    }
    log_asset_event(state, session, kind, id, text::PURGES_ITEM, &[]).await;
    Ok(())
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
    let current = writable(&state, &session, kind, id, false).await?;
    if current.is_deleted != trashed {
        guards::check_trash(&state.pool).await?;
    }
    let actor = history::actor(session.user.id, &session.own_name());
    let changed = assets::set_trashed(&state.pool, kind, id, trashed, &actor)
        .await?
        .ok_or(ApiError::NotFound)?;
    if changed {
        let action = if trashed {
            text::DELETES_ITEM
        } else {
            text::RESTORES_ITEM
        };
        log_asset_event(&state, &session, kind, id, action, &[]).await;
    }
    Ok(Json(detail(&state, &session, kind, id, false).await?))
}

fn parse_path(path: Result<Path<(AssetType, i32)>, PathRejection>) -> ApiResult<(AssetKind, i32)> {
    let Path((kind, id)) = path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    Ok((kind.into(), id))
}

/// The asset (or template), if visible, and in an entity of the session
/// (legacy `canUpdateItem`: items shared from a parent entity are
/// read-only).
async fn writable(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    template: bool,
) -> ApiResult<assets::Current> {
    let visible = assets::get(&state.pool, kind, &session.scope, id, template)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !session.scope.active().contains(&visible.entities_id) {
        return Err(ApiError::Forbidden);
    }
    assets::current(&state.pool, kind, id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// An asset (or a template) visible to the session.
async fn detail(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    template: bool,
) -> ApiResult<AssetDetail> {
    let row = assets::get(&state.pool, kind, &session.scope, id, template)
        .await?
        .ok_or(ApiError::NotFound)?;
    let in_selected_entity = session.scope.active().contains(&row.entities_id);
    let permissions = AssetPermissions {
        update: in_selected_entity && session.rights.has(kind.right(), UPDATE),
        // A template is purged, not moved to the trash.
        trash: !template && in_selected_entity && session.rights.has(kind.right(), DELETE),
        purge: in_selected_entity && session.rights.has(kind.right(), PURGE),
    };
    let specific = assets::specific(&state.pool, kind, id).await?;
    Ok(AssetDetail::from_row(
        kind,
        row,
        &specific,
        session,
        permissions,
    ))
}

/// Stored values of the given fields, validated.
fn field_values(kind: AssetKind, fields: &AssetFields) -> ApiResult<Vec<(Field, Value)>> {
    // An empty column for a type without type or model: no such field.
    let type_column = kind.type_dropdown().map_or("", |(column, _)| column);
    let model_column = kind.model_dropdown().map_or("", |(column, _)| column);
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
        ("version", "version", &fields.datacenter.version),
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
        (
            "power_supply_id",
            "phonepowersupplies_id",
            fields.power_supply_id,
        ),
        ("room_id", "dcrooms_id", fields.datacenter.room_id),
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
    let flags = [
        ("is_recursive", "is_recursive", fields.is_recursive),
        ("is_global", "is_global", fields.is_global),
    ];
    for (name, column, flag) in flags.into_iter().chain(fields.features.columns(kind)) {
        let Some(flag) = flag else { continue };
        values.push((field(name, column)?, Value::Bool(flag)));
    }

    // Numbers, kept as the legacy input filter keeps them; what it would
    // turn into another number, or the database refuse, is refused.
    if let Some(size) = &fields.size {
        if !is_decimal(size) {
            return Err(ApiError::BadRequest(
                "size: a decimal number is required".to_owned(),
            ));
        }
        let size_value = filter_decimal(size);
        if !fits_size(&size_value) {
            return Err(ApiError::BadRequest("size: out of range".to_owned()));
        }
        values.push((field("size", "size")?, Value::Text(Some(size_value))));
    }
    if let Some(memory) = &fields.memory {
        if kind == AssetKind::NetworkEquipment {
            let trimmed = memory.trim();
            let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
            let valid = trimmed.is_empty()
                || (!digits.is_empty()
                    && digits.len() <= 18
                    && digits.bytes().all(|byte| byte.is_ascii_digit()));
            if !valid {
                return Err(ApiError::BadRequest(
                    "memory: a whole number is required".to_owned(),
                ));
            }
            values.push((
                field("memory", "ram")?,
                Value::Text(Some(filter_integer(memory))),
            ));
        } else {
            let stored = to_stored(memory);
            if stored.chars().count() > MAX_TEXT_CHARS {
                return Err(ApiError::BadRequest("memory: too long".to_owned()));
            }
            values.push((field("memory", "memory_size")?, Value::Text(Some(stored))));
        }
    }
    let counters = [
        (
            "initial_page_counter",
            "init_pages_counter",
            fields.initial_page_counter,
        ),
        ("page_counter", "last_pages_counter", fields.page_counter),
    ];
    for (name, column, counter) in counters {
        let Some(counter) = counter else { continue };
        values.push((field(name, column)?, Value::Text(Some(counter.to_string()))));
    }
    datacenter_values(kind, &fields.datacenter, &mut values)?;
    Ok(values)
}

/// Fields of the racks and enclosures, validated: what the classic form
/// offers (a door orientation, 1 to 100 units, a color), whole numbers.
fn datacenter_values(
    kind: AssetKind,
    fields: &DatacenterFields,
    values: &mut Vec<(Field, Value)>,
) -> ApiResult<()> {
    let field = |name: &str, column: &str| {
        kind.field(column).ok_or_else(|| {
            ApiError::BadRequest(format!("{name}: no such field for {}", kind.slug()))
        })
    };
    let number = |value: i32| Value::Text(Some(value.to_string()));
    if let Some(position) = &fields.position {
        let stored = to_stored(position);
        if stored.chars().count() > 50 {
            return Err(ApiError::BadRequest("position: too long".to_owned()));
        }
        values.push((field("position", "position")?, Value::Text(Some(stored))));
    }
    let bounded = [
        (
            "room_orientation",
            "room_orientation",
            fields.room_orientation,
            1,
            4,
        ),
        ("number_units", "number_units", fields.number_units, 1, 100),
        (
            "power_supplies",
            "power_supplies",
            fields.power_supplies,
            0,
            i32::MAX,
        ),
    ];
    for (name, column, value, min, max) in bounded {
        let Some(value) = value else { continue };
        if !(min..=max).contains(&value) {
            return Err(ApiError::BadRequest(format!("{name}: out of range")));
        }
        values.push((field(name, column)?, number(value)));
    }
    let sizes = [
        ("width", "width", fields.width),
        ("height", "height", fields.height),
        ("depth", "depth", fields.depth),
        ("max_power", "max_power", fields.max_power),
        ("measured_power", "mesured_power", fields.measured_power),
        ("max_weight", "max_weight", fields.max_weight),
    ];
    for (name, column, value) in sizes {
        let Some(value) = value else { continue };
        let value = match value {
            // Empty, as the form posts it: stored as 0.
            None => Value::Text(Some(String::new())),
            Some(value) if value < 0 => {
                return Err(ApiError::BadRequest(format!("{name}: out of range")));
            }
            Some(value) => number(value),
        };
        values.push((field(name, column)?, value));
    }
    if let Some(color) = &fields.background_color {
        let valid = color.len() == 7
            && color.starts_with('#')
            && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(ApiError::BadRequest(
                "background_color: #RRGGBB is required".to_owned(),
            ));
        }
        values.push((
            field("background_color", "bgcolor")?,
            Value::Text(Some(color.to_lowercase())),
        ));
    }
    Ok(())
}

/// `Rack::prepareInput`: another rack outside the trash at the same place
/// of the room refuses the write (`form`: every field of the form).
async fn check_rack_position(
    state: &AppState,
    kind: AssetKind,
    form: &[(Field, Value)],
    id: Option<i32>,
) -> ApiResult<()> {
    if kind != AssetKind::Rack {
        return Ok(());
    }
    let value = |column: &str| form.iter().find(|(field, _)| field.column == column);
    let room = value("dcrooms_id").map_or(0, |(_, value)| value.id());
    let position = value("position")
        .map(|(_, value)| value.raw())
        .unwrap_or_default();
    if !rack::position_is_checked(room, &position) {
        return Ok(());
    }
    if assets::racks::position_taken(&state.pool, room, &position, id).await? {
        return Err(ApiError::PositionUnavailable(from_stored(&position)));
    }
    Ok(())
}

/// A decimal number: digits, with `.` or `,` before the decimals.
fn is_decimal(text: &str) -> bool {
    let text = text.trim();
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (integer, fraction) = unsigned.split_once(['.', ',']).unwrap_or((unsigned, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    !(integer.is_empty() && fraction.is_empty()) && digits(integer) && digits(fraction)
}

/// Fits `decimal(5,2)` once rounded to the cent like the database.
fn fits_size(value: &str) -> bool {
    if value.contains('E') {
        return value
            .parse::<f64>()
            .is_ok_and(|number| number.abs() < 999.995);
    }
    let unsigned = value.trim_start_matches('-');
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let Ok(integer) = integer.parse::<u64>() else {
        return false;
    };
    let mut decimals = fraction.bytes().map(|byte| u64::from(byte - b'0'));
    let tens = decimals.next().unwrap_or(0);
    let units = decimals.next().unwrap_or(0);
    let round_up = decimals.next().unwrap_or(0) >= 5;
    integer
        .checked_mul(100)
        .and_then(|cents| cents.checked_add(tens * 10 + units + u64::from(round_up)))
        .is_some_and(|cents| cents <= 99_999)
}

async fn check_references(state: &AppState, values: &[(Field, Value)]) -> ApiResult<()> {
    let missing = assets::references_exist(&state.pool, values).await?;
    match missing.first() {
        Some(column) => Err(ApiError::BadRequest(format!("{column}: unknown id"))),
        None => Ok(()),
    }
}

/// Event of the legacy asset forms, in the language of the session: the
/// login, then `args`.
async fn log_asset_event(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    id: i32,
    action: Source,
    args: &[&str],
) {
    let login = session.user.name.clone().unwrap_or_default();
    let mut values = vec![login.as_str()];
    values.extend_from_slice(args);
    let format = i18n::tr(&session.preferences.language, action);
    log_event(
        state,
        &events::Event {
            items_id: id,
            kind: kind.event_type(),
            service: "inventory",
            level: 4,
            message: &i18n::sprintf(&format, &values),
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
        specific: &assets::Specific,
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
            size: specific.text("size"),
            memory: decode(
                specific
                    .text("memory_size")
                    .or_else(|| specific.text("ram")),
            ),
            initial_page_counter: specific
                .text("init_pages_counter")
                .and_then(|counter| counter.parse().ok()),
            page_counter: specific
                .text("last_pages_counter")
                .and_then(|counter| counter.parse().ok()),
            power_supply: specific
                .text("phonepowersupplies_id")
                .and_then(|id| id.parse().ok())
                .and_then(|id| Ref::optional(id, specific.power_supply.clone())),
            features: Features::read(kind, specific),
            datacenter: DatacenterDetail::read(specific),
            is_deleted: row.is_deleted,
            is_dynamic: row.is_dynamic != 0,
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
            is_template: row.is_template != 0,
            template_name: if row.is_template != 0 {
                decode(row.template_name)
            } else {
                None
            },
            permissions,
            duplicates: Vec::new(),
        }
    }
}
