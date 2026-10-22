//! Places of the racks in their server room: `/assets/racks/positions`.

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use nw_db::assets::racks;
use nw_domain::{
    asset::AssetKind,
    rack,
    rights::{CREATE, UPDATE},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::Session,
    state::AppState,
};

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PositionParams {
    /// Server room.
    pub room_id: i32,
    /// Position of the rack being edited: offered although it is taken.
    pub current: Option<String>,
}

/// A free place of the room.
#[derive(Debug, Serialize, ToSchema)]
pub struct RoomPosition {
    /// Stored value, `"<column>,<row>"`.
    pub position: String,
    /// Column in letters (A, B... AA).
    pub column: String,
    pub row: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoomPositions {
    /// By column, then row.
    pub items: Vec<RoomPosition>,
}

/// Places of a server room no rack outside the trash takes, like the
/// classic form offers them (`ajax/dcroom_size.php`).
#[utoipa::path(
    get,
    path = "/api/v2/assets/racks/positions",
    tag = "assets",
    params(PositionParams),
    responses(
        (status = 200, description = "Free places", body = RoomPositions),
        (status = 400, description = "Invalid parameters", body = ErrorBody),
        (status = 401, description = "Not logged in", body = ErrorBody),
        (status = 403, description = "No create or update right on the racks", body = ErrorBody),
        (status = 404, description = "Unknown or not visible room", body = ErrorBody),
    )
)]
pub async fn positions(
    State(state): State<AppState>,
    session: Session,
    params: Result<Query<PositionParams>, QueryRejection>,
) -> ApiResult<Json<RoomPositions>> {
    let right = AssetKind::Rack.right();
    if !(session.rights.has(right, CREATE) || session.rights.has(right, UPDATE)) {
        return Err(ApiError::Forbidden);
    }
    let Query(params) = params.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let room = racks::room(&state.pool, params.room_id)
        .await?
        .filter(|room| session.scope.can_see(room.entities_id, room.is_recursive))
        .ok_or(ApiError::NotFound)?;
    let taken = racks::taken_positions(&state.pool, room.id).await?;
    let items = rack::free_positions(
        room.vis_cols,
        room.vis_rows,
        &taken,
        params.current.as_deref(),
    )
    .into_iter()
    .map(|position| RoomPosition {
        position: position.position,
        column: position.column,
        row: position.row,
    })
    .collect();
    Ok(Json(RoomPositions { items }))
}
