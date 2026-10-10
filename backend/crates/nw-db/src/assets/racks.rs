//! Racks in their server room (`glpi_dcrooms`).

use sqlx::{FromRow, MySqlPool};

/// A server room and the size of its grid.
#[derive(Debug, Clone, FromRow)]
pub struct Room {
    pub id: i32,
    pub entities_id: i32,
    pub is_recursive: bool,
    pub vis_cols: i32,
    pub vis_rows: i32,
}

pub async fn room(pool: &MySqlPool, id: i32) -> Result<Option<Room>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, entities_id, is_recursive, IFNULL(vis_cols, 0) AS vis_cols, \
         IFNULL(vis_rows, 0) AS vis_rows FROM glpi_dcrooms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Positions of the racks of a room outside the trash (`getFilled`).
pub async fn taken_positions(pool: &MySqlPool, room: i32) -> Result<Vec<String>, sqlx::Error> {
    let positions: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT position FROM glpi_racks WHERE dcrooms_id = ? AND is_deleted = 0 ORDER BY id",
    )
    .bind(room)
    .fetch_all(pool)
    .await?;
    Ok(positions.into_iter().flatten().collect())
}

/// Whether another rack outside the trash is at this position of the room
/// (`Rack::prepareInput`, compared by the database). Templates count too.
pub async fn position_taken(
    pool: &MySqlPool,
    room: i32,
    position: &str,
    except: Option<i32>,
) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM glpi_racks \
         WHERE dcrooms_id = ? AND position = ? AND is_deleted = 0 AND id <> ?",
    )
    .bind(room)
    .bind(position)
    .bind(except.unwrap_or(0))
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}
