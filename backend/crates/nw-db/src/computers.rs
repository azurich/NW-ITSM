//! Computers (`glpi_computers`).

use chrono::{DateTime, Utc};
use nw_domain::entity::EntityScope;
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use crate::scope::restrict_entities;

/// Sortable columns of the computer list.
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
            Self::Name => "c.name",
            Self::Serial => "c.serial",
            Self::InventoryNumber => "c.otherserial",
            Self::Entity => "e.completename",
            Self::Location => "l.completename",
            Self::Type => "ct.name",
            Self::Manufacturer => "m.name",
            Self::State => "s.completename",
            Self::UpdatedAt => "c.date_mod",
        }
    }
}

/// Filters, order and page of the computer list.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// Matches name, serial, inventory number or UUID.
    pub search: Option<String>,
    pub sort: Sort,
    pub descending: bool,
    pub limit: u32,
    pub offset: u32,
}

/// A computer in the list.
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
    pub computertypes_id: i32,
    pub computertype: Option<String>,
    pub manufacturers_id: i32,
    pub manufacturer: Option<String>,
    pub states_id: i32,
    pub state: Option<String>,
    pub date_mod: Option<DateTime<Utc>>,
}

/// A computer with its related names.
#[derive(Debug, Clone, FromRow)]
pub struct Detail {
    pub id: i32,
    pub name: Option<String>,
    pub serial: Option<String>,
    pub otherserial: Option<String>,
    pub uuid: Option<String>,
    pub contact: Option<String>,
    pub contact_num: Option<String>,
    pub comment: Option<String>,
    pub is_deleted: bool,
    pub is_dynamic: bool,
    pub is_recursive: bool,
    pub date_creation: Option<DateTime<Utc>>,
    pub date_mod: Option<DateTime<Utc>>,
    pub entities_id: i32,
    pub entity: Option<String>,
    pub locations_id: i32,
    pub location: Option<String>,
    pub computertypes_id: i32,
    pub computertype: Option<String>,
    pub computermodels_id: i32,
    pub computermodel: Option<String>,
    pub manufacturers_id: i32,
    pub manufacturer: Option<String>,
    pub states_id: i32,
    pub state: Option<String>,
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

const LIST_COLUMNS: &str = "c.id, c.name, c.serial, c.otherserial, \
    c.entities_id, e.completename AS entity, \
    c.locations_id, l.completename AS location, \
    c.computertypes_id, ct.name AS computertype, \
    c.manufacturers_id, m.name AS manufacturer, \
    c.states_id, s.completename AS state, \
    c.date_mod";

const LIST_JOINS: &str = " FROM glpi_computers c \
    LEFT JOIN glpi_entities e ON e.id = c.entities_id \
    LEFT JOIN glpi_locations l ON l.id = c.locations_id \
    LEFT JOIN glpi_computertypes ct ON ct.id = c.computertypes_id \
    LEFT JOIN glpi_manufacturers m ON m.id = c.manufacturers_id \
    LEFT JOIN glpi_states s ON s.id = c.states_id";

/// Lists computers visible in the scope (trash and templates excluded),
/// with the total number of matching rows.
pub async fn list(
    pool: &MySqlPool,
    scope: &EntityScope,
    query: &ListQuery,
) -> Result<(Vec<ListItem>, i64), sqlx::Error> {
    let pattern = query.search.as_deref().map(like_pattern);
    let filter = |sql: &mut QueryBuilder<'_, MySql>| {
        sql.push(" WHERE c.is_deleted = 0 AND c.is_template = 0");
        restrict_entities(sql, "c", scope);
        if let Some(pattern) = &pattern {
            sql.push(" AND (");
            for (i, column) in ["c.name", "c.serial", "c.otherserial", "c.uuid"]
                .iter()
                .enumerate()
            {
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
    count.push(LIST_JOINS);
    filter(&mut count);
    let (total,): (i64,) = count.build_query_as().fetch_one(pool).await?;

    let mut select = QueryBuilder::<MySql>::new("SELECT ");
    select.push(LIST_COLUMNS).push(LIST_JOINS);
    filter(&mut select);
    let direction = if query.descending { "DESC" } else { "ASC" };
    select.push(format_args!(
        " ORDER BY {} {direction}, c.id {direction} LIMIT ",
        query.sort.column()
    ));
    select
        .push_bind(query.limit)
        .push(" OFFSET ")
        .push_bind(query.offset);
    let items = select.build_query_as().fetch_all(pool).await?;

    Ok((items, total))
}

/// Fetches one computer if it is visible in the scope. Items in the trash
/// are returned (flagged), templates are not.
pub async fn get(
    pool: &MySqlPool,
    scope: &EntityScope,
    id: i32,
) -> Result<Option<Detail>, sqlx::Error> {
    let mut sql = QueryBuilder::<MySql>::new(
        "SELECT c.id, c.name, c.serial, c.otherserial, c.uuid, c.contact, c.contact_num, c.comment, \
                c.is_deleted, c.is_dynamic, c.is_recursive, c.date_creation, c.date_mod, \
                c.entities_id, e.completename AS entity, \
                c.locations_id, l.completename AS location, \
                c.computertypes_id, ct.name AS computertype, \
                c.computermodels_id, cm.name AS computermodel, \
                c.manufacturers_id, m.name AS manufacturer, \
                c.states_id, s.completename AS state, \
                c.users_id, u.name AS user_name, u.realname AS user_realname, u.firstname AS user_firstname, \
                c.groups_id, g.completename AS `group`, \
                c.users_id_tech, ut.name AS tech_name, ut.realname AS tech_realname, ut.firstname AS tech_firstname, \
                c.groups_id_tech, gt.completename AS tech_group \
         FROM glpi_computers c \
         LEFT JOIN glpi_entities e ON e.id = c.entities_id \
         LEFT JOIN glpi_locations l ON l.id = c.locations_id \
         LEFT JOIN glpi_computertypes ct ON ct.id = c.computertypes_id \
         LEFT JOIN glpi_computermodels cm ON cm.id = c.computermodels_id \
         LEFT JOIN glpi_manufacturers m ON m.id = c.manufacturers_id \
         LEFT JOIN glpi_states s ON s.id = c.states_id \
         LEFT JOIN glpi_users u ON u.id = c.users_id \
         LEFT JOIN glpi_groups g ON g.id = c.groups_id \
         LEFT JOIN glpi_users ut ON ut.id = c.users_id_tech \
         LEFT JOIN glpi_groups gt ON gt.id = c.groups_id_tech \
         WHERE c.is_template = 0 AND c.id = ",
    );
    sql.push_bind(id);
    restrict_entities(&mut sql, "c", scope);
    sql.build_query_as().fetch_optional(pool).await
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
