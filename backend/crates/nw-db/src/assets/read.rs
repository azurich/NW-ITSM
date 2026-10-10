//! Lists and details of the assets.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use nw_domain::{asset::AssetKind, entity::EntityScope, text::to_stored};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder, Row};

use crate::scope::restrict_entities;

/// Sortable columns of the asset lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Name,
    Serial,
    InventoryNumber,
    Entity,
    Location,
    Type,
    Manufacturer,
    State,
    UpdatedAt,
}

impl Sort {
    fn column(self) -> &'static str {
        match self {
            Self::Name => "a.name",
            Self::Serial => "a.serial",
            Self::InventoryNumber => "a.otherserial",
            Self::Entity => "e.completename",
            Self::Location => "l.completename",
            Self::Type => "t.name",
            Self::Manufacturer => "m.name",
            Self::State => "s.completename",
            Self::UpdatedAt => "a.date_mod",
        }
    }
}

/// Filters, order and page of an asset list.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// Matches name, serial number, inventory number (and UUID of computers).
    pub search: Option<String>,
    /// Items in the trash instead of the others.
    pub trash: bool,
    pub sort: Sort,
    pub descending: bool,
    pub limit: u32,
    pub offset: u32,
}

/// An asset in a list.
#[derive(Debug, Clone, FromRow)]
pub struct ListItem {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub otherserial: Option<String>,
    pub entities_id: i32,
    pub entity: Option<String>,
    pub locations_id: i32,
    pub location: Option<String>,
    pub types_id: i32,
    pub type_name: Option<String>,
    pub manufacturers_id: i32,
    pub manufacturer: Option<String>,
    pub states_id: i32,
    pub state: Option<String>,
    pub date_mod: Option<DateTime<Utc>>,
}

/// An asset with the names of its related rows.
#[derive(Debug, Clone, FromRow)]
pub struct Detail {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub otherserial: Option<String>,
    pub contact: Option<String>,
    pub contact_num: Option<String>,
    pub comment: Option<String>,
    pub uuid: Option<String>,
    pub brand: Option<String>,
    pub number_line: Option<String>,
    pub is_deleted: bool,
    pub is_dynamic: bool,
    pub is_recursive: bool,
    /// `0` or `1`: a number for every type, as only some tables have it.
    pub is_global: i64,
    pub date_creation: Option<DateTime<Utc>>,
    pub date_mod: Option<DateTime<Utc>>,
    pub entities_id: i32,
    pub entity: Option<String>,
    pub locations_id: i32,
    pub location: Option<String>,
    pub types_id: i32,
    pub type_name: Option<String>,
    pub models_id: i32,
    pub model_name: Option<String>,
    pub manufacturers_id: i32,
    pub manufacturer: Option<String>,
    pub states_id: i32,
    pub state: Option<String>,
    pub networks_id: i32,
    pub network: Option<String>,
    pub autoupdatesystems_id: i32,
    pub autoupdatesystem: Option<String>,
    pub users_id: i32,
    pub user_name: Option<String>,
    pub user_realname: Option<String>,
    pub user_firstname: Option<String>,
    pub groups_id: i32,
    pub group: Option<String>,
    pub users_id_tech: i32,
    pub tech_name: Option<String>,
    pub tech_realname: Option<String>,
    pub tech_firstname: Option<String>,
    pub groups_id_tech: i32,
    pub tech_group: Option<String>,
}

/// `a.<column>` when the type has the column, else a constant.
fn optional_column(kind: AssetKind, column: &str, default: &str) -> String {
    let present = kind.field(column).is_some() || (column == "is_global" && kind.can_be_global());
    if present {
        format!("a.{column}")
    } else {
        default.to_owned()
    }
}

fn joins(kind: AssetKind) -> String {
    let (type_column, type_table) = kind.type_dropdown();
    format!(
        " FROM {table} a \
         LEFT JOIN glpi_entities e ON e.id = a.entities_id \
         LEFT JOIN glpi_locations l ON l.id = a.locations_id \
         LEFT JOIN {type_table} t ON t.id = a.{type_column} \
         LEFT JOIN glpi_manufacturers m ON m.id = a.manufacturers_id \
         LEFT JOIN glpi_states s ON s.id = a.states_id",
        table = kind.table(),
    )
}

/// Lists the assets visible in the scope (templates excluded), with the
/// total number of matching rows.
pub async fn list(
    pool: &MySqlPool,
    kind: AssetKind,
    scope: &EntityScope,
    query: &ListQuery,
) -> Result<(Vec<ListItem>, i64), sqlx::Error> {
    // Stored text has `<` and `>` encoded: search the stored form.
    let pattern = query
        .search
        .as_deref()
        .map(|search| like_pattern(&to_stored(search)));
    let mut searched = vec!["a.name", "a.serial", "a.otherserial"];
    if kind == AssetKind::Computer {
        searched.push("a.uuid");
    }
    let joins = joins(kind);
    let filter = |sql: &mut QueryBuilder<'_, MySql>| {
        sql.push(" WHERE a.is_template = 0 AND a.is_deleted = ");
        sql.push_bind(query.trash);
        restrict_entities(sql, "a", scope);
        if let Some(pattern) = &pattern {
            sql.push(" AND (");
            for (i, column) in searched.iter().enumerate() {
                if i > 0 {
                    sql.push(" OR ");
                }
                sql.push(format_args!("{column} LIKE "));
                sql.push_bind(pattern.clone());
                sql.push(" ESCAPE '!'");
            }
            sql.push(")");
        }
    };

    let mut count = QueryBuilder::<MySql>::new("SELECT COUNT(*)");
    count.push(&joins);
    filter(&mut count);
    let (total,): (i64,) = count.build_query_as().fetch_one(pool).await?;

    let (type_column, _) = kind.type_dropdown();
    let mut select = QueryBuilder::<MySql>::new(format!(
        "SELECT a.id, a.name, a.serial, a.otherserial, \
                a.entities_id, e.completename AS entity, \
                a.locations_id, l.completename AS location, \
                a.{type_column} AS types_id, t.name AS type_name, \
                a.manufacturers_id, m.name AS manufacturer, \
                a.states_id, s.completename AS state, a.date_mod"
    ));
    select.push(&joins);
    filter(&mut select);
    let direction = if query.descending { "DESC" } else { "ASC" };
    select.push(format_args!(
        " ORDER BY {} {direction}, a.id {direction} LIMIT ",
        query.sort.column()
    ));
    select
        .push_bind(query.limit)
        .push(" OFFSET ")
        .push_bind(query.offset);
    let items = select.build_query_as().fetch_all(pool).await?;

    Ok((items, total))
}

/// Fetches one asset if it is visible in the scope. Items in the trash are
/// returned (flagged), templates are not.
pub async fn get(
    pool: &MySqlPool,
    kind: AssetKind,
    scope: &EntityScope,
    id: i32,
) -> Result<Option<Detail>, sqlx::Error> {
    let (type_column, _) = kind.type_dropdown();
    let (model_column, model_table) = kind.model_dropdown();
    let networks = optional_column(kind, "networks_id", "0");
    let update_sources = optional_column(kind, "autoupdatesystems_id", "0");
    let mut sql = QueryBuilder::<MySql>::new(format!(
        "SELECT a.id, a.name, a.serial, a.otherserial, a.contact, a.contact_num, a.comment, \
                {uuid} AS uuid, {brand} AS brand, {number_line} AS number_line, \
                a.is_deleted, a.is_dynamic, a.is_recursive, {is_global} AS is_global, \
                a.date_creation, a.date_mod, \
                a.entities_id, e.completename AS entity, \
                a.locations_id, l.completename AS location, \
                a.{type_column} AS types_id, t.name AS type_name, \
                a.{model_column} AS models_id, md.name AS model_name, \
                a.manufacturers_id, m.name AS manufacturer, \
                a.states_id, s.completename AS state, \
                {networks} AS networks_id, n.name AS network, \
                {update_sources} AS autoupdatesystems_id, au.name AS autoupdatesystem, \
                a.users_id, u.name AS user_name, u.realname AS user_realname, u.firstname AS user_firstname, \
                a.groups_id, g.completename AS `group`, \
                a.users_id_tech, ut.name AS tech_name, ut.realname AS tech_realname, ut.firstname AS tech_firstname, \
                a.groups_id_tech, gt.completename AS tech_group",
        uuid = optional_column(kind, "uuid", "NULL"),
        brand = optional_column(kind, "brand", "NULL"),
        number_line = optional_column(kind, "number_line", "NULL"),
        is_global = if kind.can_be_global() {
            "CAST(a.is_global AS SIGNED)"
        } else {
            "CAST(0 AS SIGNED)"
        },
    ));
    sql.push(joins(kind));
    sql.push(format_args!(
        " LEFT JOIN {model_table} md ON md.id = a.{model_column} \
          LEFT JOIN glpi_networks n ON n.id = {networks} \
          LEFT JOIN glpi_autoupdatesystems au ON au.id = {update_sources} \
          LEFT JOIN glpi_users u ON u.id = a.users_id \
          LEFT JOIN glpi_groups g ON g.id = a.groups_id \
          LEFT JOIN glpi_users ut ON ut.id = a.users_id_tech \
          LEFT JOIN glpi_groups gt ON gt.id = a.groups_id_tech \
          WHERE a.is_template = 0 AND a.id = "
    ));
    sql.push_bind(id);
    restrict_entities(&mut sql, "a", scope);
    sql.build_query_as().fetch_optional(pool).await
}

/// Every field of an asset as text (`1` or `0` for the flags), with the
/// name of the power supply of a phone.
#[derive(Debug, Clone, Default)]
pub struct Specific {
    pub values: HashMap<&'static str, Option<String>>,
    pub power_supply: Option<String>,
}

impl Specific {
    pub fn text(&self, column: &str) -> Option<String> {
        self.values.get(column).cloned().flatten()
    }

    /// A flag of the type, `None` when the type does not have it.
    pub fn flag(&self, column: &str) -> Option<bool> {
        self.values
            .get(column)
            .map(|value| value.as_deref() == Some("1"))
    }
}

/// Reads the fields of an asset as text. Call it once the asset is known
/// to be visible ([`get`]).
pub async fn specific(pool: &MySqlPool, kind: AssetKind, id: i32) -> Result<Specific, sqlx::Error> {
    let fields = kind.fields();
    let mut columns: Vec<String> = fields
        .iter()
        .map(|field| format!("CAST(a.{0} AS CHAR) AS {0}", field.column))
        .collect();
    let mut joins = String::new();
    let has_power_supply = kind.field("phonepowersupplies_id").is_some();
    if has_power_supply {
        columns.push("ps.name AS power_supply".to_owned());
        joins.push_str(" LEFT JOIN glpi_phonepowersupplies ps ON ps.id = a.phonepowersupplies_id");
    }
    let row = sqlx::query(&format!(
        "SELECT {} FROM {} a{joins} WHERE a.id = ?",
        columns.join(", "),
        kind.table()
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(Specific::default());
    };
    let mut specific = Specific::default();
    for field in fields {
        specific
            .values
            .insert(field.column, row.try_get(field.column)?);
    }
    if has_power_supply {
        specific.power_supply = row.try_get("power_supply")?;
    }
    Ok(specific)
}

/// `%term%` with `!` escaping the LIKE wildcards (`!` is used instead of
/// the backslash, whose meaning depends on the SQL mode).
fn like_pattern(term: &str) -> String {
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for c in term.chars() {
        if matches!(c, '!' | '%' | '_') {
            pattern.push('!');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::like_pattern;

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(like_pattern("pc-01"), "%pc-01%");
        assert_eq!(like_pattern("100%_!"), "%100!%!_!!%");
    }
}
