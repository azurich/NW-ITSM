//! Settings of the PHP features that run on asset writes and are not
//! ported yet: the API checks them to refuse writes it cannot do the same
//! way.

use std::collections::HashMap;

use nw_domain::rules::{AssetRule, Criterion};
use sqlx::MySqlPool;

/// Active `RuleAsset` rules, whatever their entity.
pub async fn active_asset_rules(pool: &MySqlPool) -> Result<Vec<AssetRule>, sqlx::Error> {
    let rules: Vec<(i32, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, `match`, `condition` FROM glpi_rules WHERE sub_type = 'RuleAsset' AND is_active = 1",
    )
    .fetch_all(pool)
    .await?;
    let criteria: Vec<(i32, Option<String>, i32, Option<String>)> = sqlx::query_as(
        "SELECT c.rules_id, c.criteria, c.`condition`, c.pattern FROM glpi_rulecriterias c \
         INNER JOIN glpi_rules r ON r.id = c.rules_id \
         WHERE r.sub_type = 'RuleAsset' AND r.is_active = 1",
    )
    .fetch_all(pool)
    .await?;
    let mut by_rule: HashMap<i32, Vec<Criterion>> = HashMap::new();
    for (rule_id, criteria, condition, pattern) in criteria {
        by_rule.entry(rule_id).or_default().push(Criterion {
            criteria: criteria.unwrap_or_default(),
            condition,
            pattern: pattern.unwrap_or_default(),
        });
    }
    Ok(rules
        .into_iter()
        .map(|(id, matching, condition)| AssetRule {
            match_all: matching.as_deref() != Some("OR"),
            condition,
            criteria: by_rule.remove(&id).unwrap_or_default(),
        })
        .collect())
}

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

/// Raw values of entity settings (`glpi_entities` columns), by entity id.
///
/// `columns` must be trusted column names.
pub async fn entity_settings(
    pool: &MySqlPool,
    columns: &[&str],
) -> Result<HashMap<i32, Vec<Option<String>>>, sqlx::Error> {
    use sqlx::Row;
    let casted: Vec<String> = columns
        .iter()
        .map(|column| format!("CAST({column} AS CHAR) AS {column}"))
        .collect();
    let rows = sqlx::query(&format!(
        "SELECT id, {} FROM glpi_entities",
        casted.join(", ")
    ))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id: i32 = row.try_get("id")?;
            let values = columns
                .iter()
                .map(|column| row.try_get::<Option<String>, _>(*column))
                .collect::<Result<_, _>>()?;
            Ok((id, values))
        })
        .collect()
}
