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
    /// Sorted column; `NULL` when the type does not have it.
    fn column(self, kind: AssetKind) -> &'static str {
        let present = match self {
            Self::Serial => kind.has_column("serial"),
            Self::InventoryNumber => kind.has_column("otherserial"),
            Self::Location => kind.has_column("locations_id"),
            Self::Type => kind.type_dropdown().is_some(),
            Self::Manufacturer => kind.has_column("manufacturers_id"),
            Self::Name | Self::Entity | Self::State | Self::UpdatedAt => true,
        };
        if !present {
            return "NULL";
        }
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
    /// `0` or `1`, numbers as only some tables have them.
    pub is_dynamic: i64,
    pub is_recursive: bool,
    pub is_global: i64,
    pub is_template: i64,
    pub template_name: Option<String>,
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
    if kind.has_column(column) {
        format!("a.{column}")
    } else {
        default.to_owned()
    }
}

/// `<alias>.<column>` of a joined table, `NULL` when the type has no join.
fn joined(present: bool, column: &str) -> &str {
    if present { column } else { "NULL" }
}

fn joins(kind: AssetKind) -> String {
    let mut sql = format!(
        " FROM {} a LEFT JOIN glpi_entities e ON e.id = a.entities_id",
        kind.table()
    );
    if kind.has_column("locations_id") {
        sql.push_str(" LEFT JOIN glpi_locations l ON l.id = a.locations_id");
    }
    if let Some((type_column, type_table)) = kind.type_dropdown() {
        sql.push_str(&format!(
            " LEFT JOIN {type_table} t ON t.id = a.{type_column}"
        ));
    }
    if kind.has_column("manufacturers_id") {
        sql.push_str(" LEFT JOIN glpi_manufacturers m ON m.id = a.manufacturers_id");
    }
    sql.push_str(" LEFT JOIN glpi_states s ON s.id = a.states_id");
    sql
}

/// Columns of a list or a detail shared by every type: `NULL` (or `0`)
/// for those a type does not have.
fn summary_columns(kind: AssetKind) -> String {
    let type_id = kind
        .type_dropdown()
        .map_or("0".to_owned(), |(column, _)| format!("a.{column}"));
    format!(
        "a.id, a.name, {serial} AS serial, {otherserial} AS otherserial, \
         a.entities_id, e.completename AS entity, \
         {locations_id} AS locations_id, {location} AS location, \
         {type_id} AS types_id, {type_name} AS type_name, \
         {manufacturers_id} AS manufacturers_id, {manufacturer} AS manufacturer, \
         a.states_id, s.completename AS state",
        serial = optional_column(kind, "serial", "NULL"),
        otherserial = optional_column(kind, "otherserial", "NULL"),
        locations_id = optional_column(kind, "locations_id", "0"),
        location = joined(kind.has_column("locations_id"), "l.completename"),
        type_name = joined(kind.type_dropdown().is_some(), "t.name"),
        manufacturers_id = optional_column(kind, "manufacturers_id", "0"),
        manufacturer = joined(kind.has_column("manufacturers_id"), "m.name"),
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
    let searched: Vec<String> = ["name", "serial", "otherserial", "uuid"]
        .into_iter()
        .filter(|column| kind.has_column(column))
        .map(|column| format!("a.{column}"))
        .collect();
    let joins = joins(kind);
    let filter = |sql: &mut QueryBuilder<'_, MySql>| {
        sql.push(" WHERE a.is_deleted = ");
        sql.push_bind(query.trash);
        if kind.has_templates() {
            sql.push(" AND a.is_template = 0");
        }
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

    let mut select =
        QueryBuilder::<MySql>::new(format!("SELECT {}, a.date_mod", summary_columns(kind)));
    select.push(&joins);
    filter(&mut select);
    let direction = if query.descending { "DESC" } else { "ASC" };
    select.push(format_args!(
        " ORDER BY {} {direction}, a.id {direction} LIMIT ",
        query.sort.column(kind)
    ));
    select
        .push_bind(query.limit)
        .push(" OFFSET ")
        .push_bind(query.offset);
    let items = select.build_query_as().fetch_all(pool).await?;

    Ok((items, total))
}

/// Fetches one asset (or template) if it is visible in the scope. Items in
/// the trash are returned (flagged).
pub async fn get(
    pool: &MySqlPool,
    kind: AssetKind,
    scope: &EntityScope,
    id: i32,
    template: bool,
) -> Result<Option<Detail>, sqlx::Error> {
    let networks = optional_column(kind, "networks_id", "0");
    let update_sources = optional_column(kind, "autoupdatesystems_id", "0");
    let users = optional_column(kind, "users_id", "0");
    let groups = optional_column(kind, "groups_id", "0");
    // The models of the datacenter items can be NULL.
    let (models, model_join) = match kind.model_dropdown() {
        Some((column, table)) => (
            format!("IFNULL(a.{column}, 0)"),
            format!(" LEFT JOIN {table} md ON md.id = a.{column}"),
        ),
        None => ("0".to_owned(), String::new()),
    };
    let mut sql = QueryBuilder::<MySql>::new(format!(
        "SELECT {summary}, {contact} AS contact, {contact_num} AS contact_num, a.comment, \
                {uuid} AS uuid, {brand} AS brand, {number_line} AS number_line, \
                a.is_deleted, CAST({is_dynamic} AS SIGNED) AS is_dynamic, a.is_recursive, \
                CAST({is_global} AS SIGNED) AS is_global, \
                CAST({is_template} AS SIGNED) AS is_template, {template_name} AS template_name, \
                a.date_creation, a.date_mod, \
                {models} AS models_id, {model_name} AS model_name, \
                {networks} AS networks_id, n.name AS network, \
                {update_sources} AS autoupdatesystems_id, au.name AS autoupdatesystem, \
                {users} AS users_id, u.name AS user_name, u.realname AS user_realname, u.firstname AS user_firstname, \
                {groups} AS groups_id, g.completename AS `group`, \
                a.users_id_tech, ut.name AS tech_name, ut.realname AS tech_realname, ut.firstname AS tech_firstname, \
                a.groups_id_tech, gt.completename AS tech_group",
        summary = summary_columns(kind),
        contact = optional_column(kind, "contact", "NULL"),
        contact_num = optional_column(kind, "contact_num", "NULL"),
        uuid = optional_column(kind, "uuid", "NULL"),
        brand = optional_column(kind, "brand", "NULL"),
        number_line = optional_column(kind, "number_line", "NULL"),
        is_dynamic = optional_column(kind, "is_dynamic", "0"),
        is_global = optional_column(kind, "is_global", "0"),
        is_template = optional_column(kind, "is_template", "0"),
        template_name = optional_column(kind, "template_name", "NULL"),
        model_name = joined(kind.model_dropdown().is_some(), "md.name"),
    ));
    sql.push(joins(kind));
    sql.push(format_args!(
        "{model_join} \
          LEFT JOIN glpi_networks n ON n.id = {networks} \
          LEFT JOIN glpi_autoupdatesystems au ON au.id = {update_sources} \
          LEFT JOIN glpi_users u ON u.id = {users} \
          LEFT JOIN glpi_groups g ON g.id = {groups} \
          LEFT JOIN glpi_users ut ON ut.id = a.users_id_tech \
          LEFT JOIN glpi_groups gt ON gt.id = a.groups_id_tech \
          WHERE a.id = "
    ));
    sql.push_bind(id);
    if kind.has_templates() {
        sql.push(" AND a.is_template = ").push_bind(template);
    } else if template {
        return Ok(None);
    }
    restrict_entities(&mut sql, "a", scope);
    sql.build_query_as().fetch_optional(pool).await
}

/// A template of a type.
#[derive(Debug, Clone, FromRow)]
pub struct TemplateSummary {
    pub id: i32,
    pub template_name: Option<String>,
    pub name: Option<String>,
    pub entities_id: i32,
    pub entity: Option<String>,
}

/// Templates visible in the scope, by name (`CommonDBTM::listTemplates`).
pub async fn list_templates(
    pool: &MySqlPool,
    kind: AssetKind,
    scope: &EntityScope,
) -> Result<Vec<TemplateSummary>, sqlx::Error> {
    let mut sql = QueryBuilder::<MySql>::new(format!(
        "SELECT a.id, a.template_name, a.name, a.entities_id, e.completename AS entity \
         FROM {} a LEFT JOIN glpi_entities e ON e.id = a.entities_id WHERE a.is_template = 1",
        kind.table()
    ));
    if !kind.has_templates() {
        return Ok(Vec::new());
    }
    restrict_entities(&mut sql, "a", scope);
    sql.push(" ORDER BY a.template_name, a.id");
    sql.build_query_as().fetch_all(pool).await
}

/// Every field of an asset as text (`1` or `0` for the flags), with the
/// name of the power supply of a phone.
#[derive(Debug, Clone, Default)]
pub struct Specific {
    pub values: HashMap<&'static str, Option<String>>,
    pub power_supply: Option<String>,
    /// Server room of a rack.
    pub room: Option<String>,
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
    let has_room = kind.field("dcrooms_id").is_some();
    if has_room {
        columns.push("r.name AS room".to_owned());
        joins.push_str(" LEFT JOIN glpi_dcrooms r ON r.id = a.dcrooms_id");
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
    if has_room {
        specific.room = row.try_get("room")?;
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

/// Stored values of every column of an asset, as text, `NULL` left out
/// (the input of a clone, `CommonDBTM::clone`). Empty when it no longer
/// exists.
pub async fn row_texts(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let columns = crate::schema::columns(&mut conn, kind.table()).await?;
    let select: Vec<String> = columns
        .iter()
        .map(|column| format!("CAST(`{column}` AS CHAR) AS `{column}`"))
        .collect();
    let row = sqlx::query(&format!(
        "SELECT {} FROM {} WHERE id = ?",
        select.join(", "),
        kind.table()
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(HashMap::new());
    };
    let mut values = HashMap::new();
    for column in columns {
        if let Some(value) = row.try_get::<Option<String>, _>(column.as_str())? {
            values.insert(column, value);
        }
    }
    Ok(values)
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
