//! Entities (`glpi_entities`).

use nw_domain::entity::EntityTree;
use sqlx::MySqlPool;

/// Loads the whole entity tree.
pub async fn tree(pool: &MySqlPool) -> Result<EntityTree, sqlx::Error> {
    let rows: Vec<(i32, i32)> = sqlx::query_as("SELECT id, entities_id FROM glpi_entities")
        .fetch_all(pool)
        .await?;
    Ok(EntityTree::new(rows))
}

/// Full name of an entity ("Root > Child"), if it exists.
pub async fn completename(pool: &MySqlPool, id: i32) -> Result<Option<String>, sqlx::Error> {
    let name: Option<(Option<String>,)> =
        sqlx::query_as("SELECT completename FROM glpi_entities WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(name.and_then(|(name,)| name))
}
