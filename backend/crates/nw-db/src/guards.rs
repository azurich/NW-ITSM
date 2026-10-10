//! Settings of the PHP features that run on asset writes and are not
//! ported yet: the API checks them to refuse writes it cannot do the same
//! way.

use sqlx::MySqlPool;

/// Whether a plugin is activated (`Plugin::ACTIVATED`): its hooks run on
/// every write of the PHP code.
pub async fn has_active_plugins(pool: &MySqlPool) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM glpi_plugins WHERE state = 1")
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}
