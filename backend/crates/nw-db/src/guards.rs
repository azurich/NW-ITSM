//! Settings of the PHP features that run on asset writes and are not
//! ported yet: the API checks them to refuse writes it cannot do the same
//! way.

use sqlx::MySqlPool;

/// Whether an active field unicity rule exists for the item type.
pub async fn has_field_unicity(pool: &MySqlPool, itemtype: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM glpi_fieldunicities WHERE itemtype = ? AND is_active = 1",
    )
    .bind(itemtype)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Whether a plugin is activated (`Plugin::ACTIVATED`): its hooks run on
/// every write of the PHP code.
pub async fn has_active_plugins(pool: &MySqlPool) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM glpi_plugins WHERE state = 1")
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}
