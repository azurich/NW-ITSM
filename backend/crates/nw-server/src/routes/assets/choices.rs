//! Values offered by the asset forms: `/assets/{type}/choices/{field}`.

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
};
use nw_db::dropdowns::{self, Table, UserRight, Visibility};
use nw_domain::{
    asset::{AssetKind, Field, FieldKind},
    rights::{CREATE, UPDATE},
    software,
    text::from_stored,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::AssetType;
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

/// Number of values returned: the forms search as the user types.
const LIMIT: u32 = 50;

/// A field of the asset forms offering a list of values.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceField {
    Location,
    State,
    Manufacturer,
    Type,
    Model,
    /// Computers, printers and network equipment only.
    Network,
    /// Computers only.
    UpdateSource,
    User,
    Group,
    TechUser,
    TechGroup,
    /// Phones only.
    PowerSupply,
    /// Racks only: server room.
    Room,
    /// Software only.
    Category,
    /// Software only: the software it is an upgrade of.
    UpgradeFrom,
    /// Software only: operating system of a version.
    VersionOperatingSystem,
    /// Software only: status of a version.
    VersionState,
}

impl ChoiceField {
    /// Column of the field; `None` when the type has neither type nor model.
    fn column(self, kind: AssetKind) -> Option<&'static str> {
        Some(match self {
            Self::Location => "locations_id",
            Self::State => "states_id",
            Self::Manufacturer => "manufacturers_id",
            Self::Type => kind.type_dropdown()?.0,
            Self::Model => kind.model_dropdown()?.0,
            Self::Network => "networks_id",
            Self::UpdateSource => "autoupdatesystems_id",
            Self::User => "users_id",
            Self::Group => "groups_id",
            Self::TechUser => "users_id_tech",
            Self::TechGroup => "groups_id_tech",
            Self::PowerSupply => "phonepowersupplies_id",
            Self::Room => "dcrooms_id",
            Self::Category => "softwarecategories_id",
            Self::UpgradeFrom => "softwares_id",
            Self::VersionOperatingSystem => "operatingsystems_id",
            Self::VersionState => "states_id",
        })
    }

    /// The field, of the type or of the versions of a software.
    fn field(self, kind: AssetKind) -> Option<Field> {
        let column = self.column(kind)?;
        match self {
            Self::VersionOperatingSystem | Self::VersionState => {
                if kind == AssetKind::Software {
                    software::version_field(column)
                } else {
                    None
                }
            }
            _ => kind.field(column),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChoiceParams {
    /// Entity of the asset being edited: one of the selected entities.
    pub entity_id: i32,
    /// Text searched in the names.
    pub search: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Choice {
    pub id: i32,
    pub label: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChoiceList {
    /// At most 50 values, sorted by label.
    pub items: Vec<Choice>,
}

/// Values of a form field for an asset of the entity: the entity's own
/// values and the recursive ones of its parents, like the legacy dropdowns.
#[utoipa::path(
    get,
    path = "/api/v2/assets/{kind}/choices/{field}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("field" = ChoiceField, Path, description = "Form field"),
        ChoiceParams,
    ),
    responses(
        (status = 200, description = "Matching values", body = ChoiceList),
        (status = 400, description = "Invalid parameters, or no such field for this type", body = ErrorBody),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No create or update right, or entity not selected", body = ErrorBody),
    )
)]
pub async fn choices(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, ChoiceField)>, PathRejection>,
    params: Result<Query<ChoiceParams>, QueryRejection>,
) -> ApiResult<Json<ChoiceList>> {
    let Path((kind, field)) =
        path.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    if !(session.rights.has(kind.right(), CREATE) || session.rights.has(kind.right(), UPDATE)) {
        return Err(ApiError::Forbidden);
    }
    let Query(params) = params.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    if !session.scope.active().contains(&params.entity_id) {
        return Err(ApiError::Forbidden);
    }
    let field_kind = field
        .field(kind)
        .ok_or_else(|| ApiError::BadRequest(format!("no such field for {}", kind.slug())))?
        .kind;
    let visibility = Visibility {
        entity: params.entity_id,
        ancestors: session
            .tree
            .ancestors_of(params.entity_id)
            .into_iter()
            .collect(),
    };
    let search = params.search.as_deref();

    let items = match field_kind {
        FieldKind::User => {
            // Legacy forms: anybody as user, ticket technicians as
            // technician in charge.
            let right = if matches!(field, ChoiceField::TechUser) {
                UserRight::OwnTicket
            } else {
                UserRight::All
            };
            dropdowns::search_users(&state.pool, right, Some(&visibility), search, LIMIT)
                .await?
                .into_iter()
                .map(|user| Choice {
                    id: user.id,
                    label: session.user_name(
                        user.id,
                        user.name.as_deref(),
                        user.realname.as_deref(),
                        user.firstname.as_deref(),
                    ),
                })
                .collect()
        }
        FieldKind::Dropdown { table, tree } => {
            let condition = match field {
                ChoiceField::State => Some(format!("{} = 1", kind.state_visibility_column())),
                ChoiceField::VersionState => Some("is_visible_softwareversion = 1".to_owned()),
                ChoiceField::Group => Some("is_itemgroup = 1".to_owned()),
                ChoiceField::TechGroup => Some("is_assign = 1".to_owned()),
                // Not the trash nor the templates (`getDropdownValue`).
                ChoiceField::UpgradeFrom => Some("is_deleted = 0 AND is_template = 0".to_owned()),
                _ => None,
            };
            let table = Table {
                name: table,
                tree,
                entity_scoped: ENTITY_SCOPED.contains(&table),
            };
            dropdowns::search(
                &state.pool,
                table,
                condition.as_deref(),
                Some(&visibility),
                search,
                LIMIT,
            )
            .await?
            .into_iter()
            .map(|choice| Choice {
                id: choice.id,
                label: from_stored(&choice.label.unwrap_or_default()),
            })
            .collect()
        }
        FieldKind::Text
        | FieldKind::Bool
        | FieldKind::Integer
        | FieldKind::Decimal
        | FieldKind::Number => {
            return Err(ApiError::BadRequest("not a list field".to_owned()));
        }
    };
    Ok(Json(ChoiceList { items }))
}

/// Dropdown tables of the asset forms having an entity.
const ENTITY_SCOPED: [&str; 5] = [
    "glpi_locations",
    "glpi_states",
    "glpi_groups",
    "glpi_dcrooms",
    "glpi_softwares",
];
