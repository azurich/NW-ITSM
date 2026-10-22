//! Global settings stored in `glpi_configs`.

use std::collections::HashMap;

use sqlx::{MySql, MySqlPool, QueryBuilder};

/// Reads values of the `core` context. Missing names are absent from the map.
pub async fn core_values(
    pool: &MySqlPool,
    names: &[&str],
) -> Result<HashMap<String, String>, sqlx::Error> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT name, value FROM glpi_configs WHERE context = 'core' AND name IN (",
    );
    let mut list = query.separated(", ");
    for name in names {
        list.push_bind(*name);
    }
    query.push(")");

    let rows: Vec<(Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(name, value)| Some((name?, value.unwrap_or_default())))
        .collect())
}
