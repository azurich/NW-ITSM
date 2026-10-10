//! Unicity of fields (`FieldUnicity`): criteria, blacklisted values and
//! duplicate items.

use std::collections::HashSet;

use nw_domain::asset::AssetKind;
use nw_domain::unicity::{UnicityConfig, applicable};
use sqlx::{MySql, MySqlPool, QueryBuilder};

/// The criteria used for an item of the type in the entity
/// (`FieldUnicity::getUnicityFieldsConfig`): active, of the entity or
/// recursive in a parent, those of a single entity.
pub async fn configs(
    pool: &MySqlPool,
    itemtype: &str,
    entity: i32,
    ancestors: &[i32],
) -> Result<Vec<UnicityConfig>, sqlx::Error> {
    let mut sql = QueryBuilder::<MySql>::new(
        "SELECT id, entities_id, is_recursive, fields, action_refuse, action_notify \
         FROM glpi_fieldunicities WHERE is_active = 1 AND itemtype = ",
    );
    sql.push_bind(itemtype);
    push_visible(&mut sql, entity, ancestors);
    let rows: Vec<(i32, i32, bool, Option<String>, bool, bool)> =
        sql.build_query_as().fetch_all(pool).await?;
    Ok(applicable(
        rows.into_iter()
            .map(
                |(id, entities_id, is_recursive, fields, action_refuse, action_notify)| {
                    UnicityConfig {
                        id,
                        entities_id,
                        is_recursive,
                        fields: fields
                            .unwrap_or_default()
                            .split(',')
                            .map(str::to_owned)
                            .collect(),
                        action_refuse,
                        action_notify,
                    }
                },
            )
            .collect(),
    ))
}

/// `Fieldblacklist::isFieldBlacklisted`: a value never compared.
pub async fn is_blacklisted(
    pool: &MySqlPool,
    itemtype: &str,
    entity: i32,
    ancestors: &[i32],
    field: &str,
    value: &str,
) -> Result<bool, sqlx::Error> {
    let mut sql =
        QueryBuilder::<MySql>::new("SELECT COUNT(*) FROM glpi_fieldblacklists WHERE itemtype = ");
    sql.push_bind(itemtype);
    sql.push(" AND field = ").push_bind(field);
    sql.push(" AND value = ").push_bind(value);
    push_visible(&mut sql, entity, ancestors);
    let (count,): (i64,) = sql.build_query_as().fetch_one(pool).await?;
    Ok(count > 0)
}

/// `entities_id = <entity>` or recursive in a parent.
fn push_visible(sql: &mut QueryBuilder<'_, MySql>, entity: i32, ancestors: &[i32]) {
    sql.push(" AND (entities_id = ").push_bind(entity);
    if !ancestors.is_empty() {
        sql.push(" OR (is_recursive = 1 AND entities_id IN (");
        let mut list = sql.separated(", ");
        for id in ancestors {
            list.push_bind(*id);
        }
        sql.push("))");
    }
    sql.push(")");
}

/// Columns of a table.
pub async fn columns(pool: &MySqlPool, table: &str) -> Result<HashSet<String>, sqlx::Error> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT CAST(COLUMN_NAME AS CHAR) FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
    )
    .bind(table)
    .fetch_all(pool)
    .await?;
    Ok(names.into_iter().collect())
}

/// An item repeating the values of the one written.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Double {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub entities_id: i32,
    pub is_recursive: bool,
    /// Full name of the entity.
    pub entity: Option<String>,
    pub is_deleted: bool,
}

/// Items of the table with these values (compared by the database, like
/// the PHP code: without case nor trailing spaces), in the entities,
/// templates excepted, trash included. `criteria` columns must exist.
pub async fn doubles(
    pool: &MySqlPool,
    kind: AssetKind,
    criteria: &[(String, String)],
    entities: &[i32],
    except: Option<i32>,
) -> Result<Vec<Double>, sqlx::Error> {
    // `getField` of a column the type does not have: `NOT_AVAILABLE`.
    let serial = if kind.has_column("serial") {
        "a.serial"
    } else {
        "'N/A'"
    };
    let templates = if kind.has_templates() {
        "a.is_template = 0"
    } else {
        "1"
    };
    let mut sql = QueryBuilder::<MySql>::new(format!(
        "SELECT a.id, a.name, {serial} AS serial, a.entities_id, a.is_recursive, \
         e.completename AS entity, a.is_deleted \
         FROM {} a LEFT JOIN glpi_entities e ON e.id = a.entities_id WHERE {templates}",
        kind.table()
    ));
    for (column, value) in criteria {
        sql.push(format_args!(" AND a.`{}` = ", column.replace('`', "")));
        sql.push_bind(value.clone());
    }
    sql.push(" AND a.entities_id IN (");
    let mut list = sql.separated(", ");
    for id in entities {
        list.push_bind(*id);
    }
    sql.push(")");
    if let Some(id) = except {
        sql.push(" AND a.id <> ").push_bind(id);
    }
    sql.push(" ORDER BY a.id");
    sql.build_query_as().fetch_all(pool).await
}

/// Value of a column of an item, as text.
pub async fn stored_value(
    pool: &MySqlPool,
    table: &str,
    column: &str,
    id: i32,
) -> Result<Option<String>, sqlx::Error> {
    let value: Option<Option<String>> = sqlx::query_scalar(&format!(
        "SELECT CAST(`{}` AS CHAR) FROM {table} WHERE id = ?",
        column.replace('`', "")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(value.flatten())
}

/// Whether a duplicate would send a notification
/// (`NotificationEvent::raiseEvent('refuse', FieldUnicity)`): notifications
/// on, a mode active, and an active notification for the event.
pub async fn sends_notification(
    pool: &MySqlPool,
    entity: i32,
    ancestors: &[i32],
) -> Result<bool, sqlx::Error> {
    let settings = crate::config::core_values(
        pool,
        &[
            "use_notifications",
            "notifications_mailing",
            "notifications_ajax",
        ],
    )
    .await?;
    let on = |name: &str| {
        settings
            .get(name)
            .is_some_and(|value| value != "0" && !value.is_empty())
    };
    if !on("use_notifications") || !(on("notifications_mailing") || on("notifications_ajax")) {
        return Ok(false);
    }
    let mut sql = QueryBuilder::<MySql>::new(
        "SELECT COUNT(*) FROM glpi_notifications \
         WHERE is_active = 1 AND itemtype = 'FieldUnicity' AND event = 'refuse'",
    );
    push_visible(&mut sql, entity, ancestors);
    let (count,): (i64,) = sql.build_query_as().fetch_one(pool).await?;
    Ok(count > 0)
}

/// Field lists of the active criteria of a type, in every entity.
pub async fn active_fields(
    pool: &MySqlPool,
    itemtype: &str,
) -> Result<Vec<Vec<String>>, sqlx::Error> {
    let fields: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT fields FROM glpi_fieldunicities WHERE is_active = 1 AND itemtype = ?",
    )
    .bind(itemtype)
    .fetch_all(pool)
    .await?;
    Ok(fields
        .into_iter()
        .map(|fields| {
            fields
                .unwrap_or_default()
                .split(',')
                .map(str::to_owned)
                .collect()
        })
        .collect())
}
