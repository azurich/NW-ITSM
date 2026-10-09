//! Computers (read only).

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    routing::get,
};
use chrono::{DateTime, Utc};
use nw_db::computers::{self, ListQuery, Sort};
use nw_domain::{fk::fk, naming::user_display_name, rights::READ};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

/// Legacy right module of computers.
const RIGHT: &str = "computer";
const DEFAULT_PER_PAGE: u32 = 20;
const MAX_PER_PAGE: u32 = 100;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/computers", get(list))
        .route("/computers/{id}", get(get_one))
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
    /// Text searched in the name, serial number, inventory number and UUID.
    pub search: Option<String>,
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
        fk(id).map(|id| Self { id, name })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ComputerSummary {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub inventory_number: Option<String>,
    pub entity: Ref,
    pub location: Option<Ref>,
    #[serde(rename = "type")]
    pub computer_type: Option<Ref>,
    pub manufacturer: Option<Ref>,
    pub state: Option<Ref>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ComputerPage {
    pub items: Vec<ComputerSummary>,
    pub page: u32,
    pub per_page: u32,
    /// Number of matching computers on every page.
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ComputerDetail {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub inventory_number: Option<String>,
    pub uuid: Option<String>,
    pub contact: Option<String>,
    pub contact_number: Option<String>,
    pub comment: Option<String>,
    /// In the trash.
    pub is_deleted: bool,
    /// Updated by an inventory tool.
    pub is_dynamic: bool,
    /// Visible in sub-entities.
    pub is_recursive: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub entity: Ref,
    pub location: Option<Ref>,
    #[serde(rename = "type")]
    pub computer_type: Option<Ref>,
    pub model: Option<Ref>,
    pub manufacturer: Option<Ref>,
    pub state: Option<Ref>,
    pub user: Option<Ref>,
    pub group: Option<Ref>,
    pub tech_user: Option<Ref>,
    pub tech_group: Option<Ref>,
}

/// Lists the computers visible to the session (trash and templates excluded).
#[utoipa::path(
    get,
    path = "/api/v2/computers",
    tag = "assets",
    params(ListParams),
    responses(
        (status = 200, description = "One page of computers", body = ComputerPage),
        (status = 400, description = "Invalid parameters", body = ErrorBody),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on computers", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    params: Result<Query<ListParams>, QueryRejection>,
) -> ApiResult<Json<ComputerPage>> {
    session.require(RIGHT, READ)?;
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
        sort: params.sort.unwrap_or_default().into(),
        descending: matches!(params.order, Some(SortOrder::Desc)),
        limit: per_page,
        offset: (page - 1).saturating_mul(per_page),
    };
    let (rows, total) = computers::list(&state.pool, &session.scope, &query).await?;

    Ok(Json(ComputerPage {
        items: rows.into_iter().map(ComputerSummary::from).collect(),
        page,
        per_page,
        total,
    }))
}

/// Returns a computer visible to the session, including one in the trash.
#[utoipa::path(
    get,
    path = "/api/v2/computers/{id}",
    tag = "assets",
    params(("id" = i32, Path, description = "Computer id")),
    responses(
        (status = 200, description = "The computer", body = ComputerDetail),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read right on computers", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
    )
)]
pub async fn get_one(
    State(state): State<AppState>,
    session: Session,
    id: Result<Path<i32>, PathRejection>,
) -> ApiResult<Json<ComputerDetail>> {
    session.require(RIGHT, READ)?;
    let Path(id) = id.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let row = computers::get(&state.pool, &session.scope, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row.into()))
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

impl From<computers::ListItem> for ComputerSummary {
    fn from(row: computers::ListItem) -> Self {
        Self {
            id: row.id,
            name: row.name,
            serial: row.serial,
            inventory_number: row.otherserial,
            entity: Ref {
                id: row.entities_id,
                name: row.entity,
            },
            location: Ref::optional(row.locations_id, row.location),
            computer_type: Ref::optional(row.computertypes_id, row.computertype),
            manufacturer: Ref::optional(row.manufacturers_id, row.manufacturer),
            state: Ref::optional(row.states_id, row.state),
            updated_at: row.date_mod,
        }
    }
}

impl From<computers::Detail> for ComputerDetail {
    fn from(row: computers::Detail) -> Self {
        let user_name =
            |login: Option<String>, realname: Option<String>, firstname: Option<String>| {
                login.map(|login| {
                    user_display_name(Some(&login), realname.as_deref(), firstname.as_deref())
                })
            };
        Self {
            id: row.id,
            name: row.name,
            serial: row.serial,
            inventory_number: row.otherserial,
            uuid: row.uuid,
            contact: row.contact,
            contact_number: row.contact_num,
            comment: row.comment,
            is_deleted: row.is_deleted,
            is_dynamic: row.is_dynamic,
            is_recursive: row.is_recursive,
            created_at: row.date_creation,
            updated_at: row.date_mod,
            entity: Ref {
                id: row.entities_id,
                name: row.entity,
            },
            location: Ref::optional(row.locations_id, row.location),
            computer_type: Ref::optional(row.computertypes_id, row.computertype),
            model: Ref::optional(row.computermodels_id, row.computermodel),
            manufacturer: Ref::optional(row.manufacturers_id, row.manufacturer),
            state: Ref::optional(row.states_id, row.state),
            user: Ref::optional(
                row.users_id,
                user_name(row.user_name, row.user_realname, row.user_firstname),
            ),
            group: Ref::optional(row.groups_id, row.group),
            tech_user: Ref::optional(
                row.users_id_tech,
                user_name(row.tech_name, row.tech_realname, row.tech_firstname),
            ),
            tech_group: Ref::optional(row.groups_id_tech, row.tech_group),
        }
    }
}
