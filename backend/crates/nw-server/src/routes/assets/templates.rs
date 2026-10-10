//! Templates of the assets, under `/assets/{type}/templates`: listed,
//! created, changed and purged like with the legacy forms
//! (`withtemplate=1`), and the items created from them (`withtemplate=2`,
//! `CommonDBTM::clone`).

use axum::{
    Json,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
};
use nw_db::assets;
use nw_domain::{
    asset::{AssetKind, Field, Value},
    rights::{CREATE, READ, UPDATE},
    text::to_stored,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    AssetDetail, AssetFields, AssetType, Creation, MAX_TEXT_CHARS, Ref, create_asset, decode,
    detail, field_values, nullable_text, parse_path, purge_asset, update_asset,
};
use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

/// A template in the list.
#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateSummary {
    pub id: i32,
    pub template_name: Option<String>,
    /// Name given to the items created from it.
    pub name: Option<String>,
    pub entity: Ref,
}

/// The templates of a type visible to the session.
#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateList {
    /// By name.
    pub items: Vec<TemplateSummary>,
    /// The session may create templates, and items from them.
    pub can_create: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TemplateCreation {
    /// Entity of the new template: one of the selected entities.
    pub entity_id: i32,
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub template_name: Option<String>,
    /// The fields given to the items created from the template.
    #[serde(flatten)]
    pub fields: AssetFields,
}

/// Fields of a template. Absent fields are left unchanged; `null` clears a
/// field.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TemplateFields {
    #[serde(default, deserialize_with = "nullable_text")]
    #[schema(value_type = Option<String>)]
    pub template_name: Option<String>,
    #[serde(flatten)]
    pub fields: AssetFields,
}

/// Lists the templates of a type visible to the session, by name
/// (`CommonDBTM::listTemplates`).
#[utoipa::path(
    get,
    operation_id = "list_templates",
    path = "/api/v2/assets/{kind}/templates",
    tag = "assets",
    params(("kind" = AssetType, Path, description = "Type of asset")),
    responses(
        (status = 200, description = "The templates", body = TemplateList),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No read nor create right on this type", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    kind: Result<Path<AssetType>, PathRejection>,
) -> ApiResult<Json<TemplateList>> {
    let Path(kind) = kind.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    // To manage them, or to choose one for a new item.
    let can_create = session.rights.has(kind.right(), CREATE);
    if !can_create && !session.rights.has(kind.right(), READ) {
        return Err(ApiError::Forbidden);
    }
    let rows = assets::list_templates(&state.pool, kind, &session.scope).await?;
    Ok(Json(TemplateList {
        items: rows
            .into_iter()
            .map(|row| TemplateSummary {
                id: row.id,
                template_name: decode(row.template_name),
                name: decode(row.name),
                entity: Ref {
                    id: row.entities_id,
                    name: decode(row.entity),
                },
            })
            .collect(),
        can_create,
    }))
}

/// Returns a template visible to the session.
#[utoipa::path(
    get,
    operation_id = "get_template",
    path = "/api/v2/assets/{kind}/templates/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Template id"),
    ),
    responses(
        (status = 200, description = "The template", body = AssetDetail),
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
    Ok(Json(detail(&state, &session, kind, id, true).await?))
}

/// Creates a template in one of the selected entities. Its fields are not
/// checked for unicity, and its status fills no date.
#[utoipa::path(
    post,
    operation_id = "create_template",
    path = "/api/v2/assets/{kind}/templates",
    tag = "assets",
    params(("kind" = AssetType, Path, description = "Type of asset")),
    request_body = TemplateCreation,
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
    payload: Result<Json<TemplateCreation>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<AssetDetail>)> {
    let Path(kind) = kind.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let kind = AssetKind::from(kind);
    session.require(kind.right(), CREATE)?;
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let mut requested = field_values(kind, &request.fields)?;
    requested.extend(template_name(kind, request.template_name.as_deref())?);
    let created = create_asset(
        &state,
        &session,
        kind,
        request.entity_id,
        requested,
        Creation::Template,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Updates a template of a selected entity. Its fields are not checked
/// for unicity.
#[utoipa::path(
    patch,
    operation_id = "update_template",
    path = "/api/v2/assets/{kind}/templates/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Template id"),
    ),
    request_body = TemplateFields,
    responses(
        (status = 200, description = "Updated", body = AssetDetail),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No update right, or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Items of other entities use the shared template (`sharing_in_use`), or must be changed in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    payload: Result<Json<TemplateFields>, JsonRejection>,
) -> ApiResult<Json<AssetDetail>> {
    let (kind, id) = parse_path(path)?;
    session.require(kind.right(), UPDATE)?;
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let mut values = field_values(kind, &request.fields)?;
    values.extend(template_name(kind, request.template_name.as_deref())?);
    Ok(Json(
        update_asset(&state, &session, kind, id, values, true).await?,
    ))
}

/// Deletes a template for good, with its history and the rows the PHP
/// code deletes with it (contracts, documents, infocom...). Linked data
/// whose purge is not ported leaves it to the classic interface.
#[utoipa::path(
    delete,
    operation_id = "purge_template",
    path = "/api/v2/assets/{kind}/templates/{id}",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Template id"),
    ),
    responses(
        (status = 204, description = "Deleted for good"),
        (status = 403, description = "No purge right (on the type, or on its infocom), or entity not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible", body = ErrorBody),
        (status = 409, description = "Must be done in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn purge(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
) -> ApiResult<StatusCode> {
    let (kind, id) = parse_path(path)?;
    purge_asset(&state, &session, kind, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Creates an item from a template, in the entity of the template (one of
/// the selected entities). The fields not given take the values of the
/// template; its other columns are copied, and its infocom, contracts and
/// documents cloned for the item. Other linked data (components, network
/// ports, software...) leaves it to the classic interface.
#[utoipa::path(
    post,
    operation_id = "create_from_template",
    path = "/api/v2/assets/{kind}/templates/{id}/items",
    tag = "assets",
    params(
        ("kind" = AssetType, Path, description = "Type of asset"),
        ("id" = i32, Path, description = "Template id"),
    ),
    request_body = AssetFields,
    responses(
        (status = 201, description = "Created", body = AssetDetail),
        (status = 400, description = "Invalid fields", body = ErrorBody),
        (status = 403, description = "No create right, or entity of the template not selected", body = ErrorBody),
        (status = 404, description = "Unknown or not visible template", body = ErrorBody),
        (status = 409, description = "Another item has the same values (`duplicate`), or must be created in the classic interface (`legacy_required`)", body = ErrorBody),
    )
)]
pub async fn create_item(
    State(state): State<AppState>,
    session: Session,
    path: Result<Path<(AssetType, i32)>, PathRejection>,
    payload: Result<Json<AssetFields>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<AssetDetail>)> {
    let (kind, id) = parse_path(path)?;
    session.require(kind.right(), CREATE)?;
    let Json(fields) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let visible = assets::get(&state.pool, kind, &session.scope, id, true)
        .await?
        .ok_or(ApiError::NotFound)?;
    let template = assets::current(&state.pool, kind, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let requested = field_values(kind, &fields)?;
    let created = create_asset(
        &state,
        &session,
        kind,
        visible.entities_id,
        requested,
        Creation::FromTemplate(template),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// The name of a template, validated.
fn template_name(kind: AssetKind, name: Option<&str>) -> ApiResult<Option<(Field, Value)>> {
    let Some(name) = name else {
        return Ok(None);
    };
    let stored = to_stored(name);
    if stored.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::BadRequest("template_name: too long".to_owned()));
    }
    Ok(Some((
        kind.template_name_field(),
        Value::Text(Some(stored)),
    )))
}
