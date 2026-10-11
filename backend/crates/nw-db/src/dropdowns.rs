//! Values offered by the forms (dropdowns).

use nw_domain::text::to_stored;
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

/// A dropdown value.
#[derive(Debug, Clone, FromRow)]
pub struct Choice {
    pub id: i32,
    /// Name, or full name for trees (stored form).
    pub label: Option<String>,
}

/// A user that can be chosen.
#[derive(Debug, Clone, FromRow)]
pub struct UserChoice {
    pub id: i32,
    pub name: Option<String>,
    pub realname: Option<String>,
    pub firstname: Option<String>,
}

/// Values visible from an entity: its own, and the recursive ones of its
/// parents (legacy `getEntitiesRestrictCriteria(..., true)`).
#[derive(Debug, Clone)]
pub struct Visibility {
    pub entity: i32,
    pub ancestors: Vec<i32>,
}

/// A dropdown table.
#[derive(Debug, Clone, Copy)]
pub struct Table {
    /// Trusted table name.
    pub name: &'static str,
    /// Tree dropdown, labelled by its full name.
    pub tree: bool,
    /// Has `entities_id` and `is_recursive`.
    pub entity_scoped: bool,
}

/// Searches a dropdown table. `condition` must be a trusted SQL condition.
pub async fn search(
    pool: &MySqlPool,
    table: Table,
    condition: Option<&str>,
    visibility: Option<&Visibility>,
    search: Option<&str>,
    limit: u32,
) -> Result<Vec<Choice>, sqlx::Error> {
    let label = if table.tree { "completename" } else { "name" };
    let mut sql = QueryBuilder::<MySql>::new(format!(
        "SELECT id, {label} AS label FROM {} WHERE 1 = 1",
        table.name
    ));
    if let Some(condition) = condition {
        sql.push(format_args!(" AND {condition}"));
    }
    if let (true, Some(visibility)) = (table.entity_scoped, visibility) {
        push_visibility(&mut sql, "entities_id", "is_recursive", visibility);
    }
    if let Some(search) = search.filter(|search| !search.trim().is_empty()) {
        sql.push(format_args!(" AND {label} LIKE "));
        sql.push_bind(like_pattern(search.trim()));
        sql.push(" ESCAPE '!'");
    }
    sql.push(format_args!(" ORDER BY {label}, id LIMIT "));
    sql.push_bind(limit);
    sql.build_query_as().fetch_all(pool).await
}

/// Which users can be chosen (legacy `User::getSqlSearchResult` rights).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRight {
    /// Any user with a profile in the entity (`all`).
    All,
    /// Users that can be assigned tickets, through a profile of the
    /// standard interface (`own_ticket`).
    OwnTicket,
}

/// `Ticket::OWN`.
const TICKET_OWN: i32 = 32_768;

/// Active users with a profile visible from the entity.
pub async fn search_users(
    pool: &MySqlPool,
    right: UserRight,
    visibility: Option<&Visibility>,
    search: Option<&str>,
    limit: u32,
) -> Result<Vec<UserChoice>, sqlx::Error> {
    let mut sql = QueryBuilder::<MySql>::new(
        "SELECT DISTINCT u.id, u.name, u.realname, u.firstname FROM glpi_users u \
         INNER JOIN glpi_profiles_users pu ON pu.users_id = u.id",
    );
    if right == UserRight::OwnTicket {
        sql.push(
            " INNER JOIN glpi_profiles p ON p.id = pu.profiles_id AND p.interface = 'central' \
              INNER JOIN glpi_profilerights pr ON pr.profiles_id = p.id AND pr.name = 'ticket' \
              AND (pr.rights & ",
        );
        sql.push_bind(TICKET_OWN);
        sql.push(") <> 0");
    }
    sql.push(
        " WHERE u.is_active = 1 AND u.is_deleted = 0 \
         AND (u.begin_date IS NULL OR u.begin_date < NOW()) \
         AND (u.end_date IS NULL OR u.end_date > NOW())",
    );
    if let Some(visibility) = visibility {
        push_visibility(&mut sql, "pu.entities_id", "pu.is_recursive", visibility);
    }
    if let Some(search) = search.filter(|search| !search.trim().is_empty()) {
        let pattern = like_pattern(search.trim());
        sql.push(" AND (");
        for (i, column) in ["u.name", "u.realname", "u.firstname"].iter().enumerate() {
            if i > 0 {
                sql.push(" OR ");
            }
            sql.push(format_args!("{column} LIKE "));
            sql.push_bind(pattern.clone());
            sql.push(" ESCAPE '!'");
        }
        sql.push(")");
    }
    sql.push(" ORDER BY u.realname, u.firstname, u.name LIMIT ");
    sql.push_bind(limit);
    sql.build_query_as().fetch_all(pool).await
}

fn push_visibility(
    sql: &mut QueryBuilder<'_, MySql>,
    entity_column: &str,
    recursive_column: &str,
    visibility: &Visibility,
) {
    sql.push(format_args!(" AND ({entity_column} = "));
    sql.push_bind(visibility.entity);
    if !visibility.ancestors.is_empty() {
        sql.push(format_args!(
            " OR ({recursive_column} = 1 AND {entity_column} IN ("
        ));
        let mut ancestors = sql.separated(", ");
        for id in &visibility.ancestors {
            ancestors.push_bind(*id);
        }
        sql.push("))");
    }
    sql.push(")");
}

/// `%term%` on the stored form, with `!` escaping the LIKE wildcards.
fn like_pattern(term: &str) -> String {
    let stored = to_stored(term);
    let mut pattern = String::with_capacity(stored.len() + 2);
    pattern.push('%');
    for c in stored.chars() {
        if matches!(c, '!' | '%' | '_') {
            pattern.push('!');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}
