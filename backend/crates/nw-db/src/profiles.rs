//! Profiles granted to users (`glpi_profiles_users`, `glpi_profilerights`).

use sqlx::{FromRow, MySqlPool};

/// One profile granted to a user on one entity.
#[derive(Debug, Clone, FromRow)]
pub struct Grant {
    pub profile_id: i32,
    pub profile_name: Option<String>,
    pub interface: Option<String>,
    pub entity_id: i32,
    pub is_recursive: bool,
}

/// Every grant of a user, ordered like the legacy session (profile name,
/// then entity full name).
pub async fn grants_for_user(pool: &MySqlPool, user_id: i32) -> Result<Vec<Grant>, sqlx::Error> {
    sqlx::query_as(
        "SELECT pu.profiles_id AS profile_id, p.name AS profile_name, p.interface, \
                pu.entities_id AS entity_id, pu.is_recursive \
         FROM glpi_profiles_users pu \
         INNER JOIN glpi_profiles p ON p.id = pu.profiles_id \
         LEFT JOIN glpi_entities e ON e.id = pu.entities_id \
         WHERE pu.users_id = ? \
         ORDER BY p.name, e.completename",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// `(module, bitmask)` rights of a profile.
pub async fn rights(pool: &MySqlPool, profile_id: i32) -> Result<Vec<(String, i32)>, sqlx::Error> {
    let rows: Vec<(Option<String>, i32)> =
        sqlx::query_as("SELECT name, rights FROM glpi_profilerights WHERE profiles_id = ?")
            .bind(profile_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(name, rights)| Some((name?, rights)))
        .collect())
}
