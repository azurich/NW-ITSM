//! Purge of an asset: the row, its history and the linked rows the PHP
//! code deletes with it (`nw_domain::purge`).

use nw_domain::{
    asset::AssetKind,
    purge::{Blocker, Link, Owner},
};
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder};

/// `WHERE` condition of rows pointing to the asset.
fn push_owner(sql: &mut QueryBuilder<'_, MySql>, kind: AssetKind, id: i32, owner: Owner) {
    match (owner, kind) {
        (Owner::Computer, _) | (Owner::Connection, AssetKind::Computer) => {
            sql.push("computers_id = ").push_bind(id);
            if owner == Owner::Connection {
                // `getSQLCriteriaToSearchForItem`: both sides.
                sql.push(" OR (itemtype = ")
                    .push_bind(kind.itemtype())
                    .push(" AND items_id = ")
                    .push_bind(id)
                    .push(")");
            }
        }
        (Owner::Item | Owner::Connection, _) => {
            sql.push("itemtype = ")
                .push_bind(kind.itemtype())
                .push(" AND items_id = ")
                .push_bind(id);
        }
    }
}

async fn exists(pool: &MySqlPool, mut sql: QueryBuilder<'_, MySql>) -> Result<bool, sqlx::Error> {
    sql.push(" LIMIT 1");
    let found: Option<i64> = sql.build_query_scalar().fetch_optional(pool).await?;
    Ok(found.is_some())
}

/// Tables of the linked rows whose purge is not ported: the purge is then
/// left to the classic interface. `autoclean`: the settings clean the
/// items disconnected by a purge (`is_*_autoclean`, `state_autoclean_mode`),
/// which updates them like a form.
pub async fn purge_blockers(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    autoclean: bool,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut found = present(pool, kind, id, kind.purge_blockers()).await?;
    if autoclean && kind != AssetKind::NetworkEquipment {
        let mut sql = QueryBuilder::new("SELECT 1 FROM glpi_computers_items WHERE ");
        push_owner(&mut sql, kind, id, Owner::Connection);
        if exists(pool, sql).await? {
            found.push("glpi_computers_items");
        }
    }
    Ok(found)
}

/// Tables of the blockers having rows of the asset.
pub(super) async fn present(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    blockers: Vec<Blocker>,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut found = Vec::new();
    for blocker in blockers {
        let (name, sql) = match blocker {
            Blocker::Item(table) => {
                let mut sql = QueryBuilder::new(format!("SELECT 1 FROM {table} WHERE "));
                push_owner(&mut sql, kind, id, Owner::Item);
                (table, sql)
            }
            Blocker::Column(table, column) => {
                let mut sql = QueryBuilder::new(format!("SELECT 1 FROM {table} WHERE {column} = "));
                sql.push_bind(id);
                (table, sql)
            }
            Blocker::Reservations => {
                let mut sql = QueryBuilder::new(
                    "SELECT 1 FROM glpi_reservations r \
                     INNER JOIN glpi_reservationitems ri ON ri.id = r.reservationitems_id WHERE ri.",
                );
                sql.push("itemtype = ")
                    .push_bind(kind.itemtype())
                    .push(" AND ri.items_id = ")
                    .push_bind(id);
                ("glpi_reservations", sql)
            }
            Blocker::Impact => {
                let mut sql = QueryBuilder::new(
                    "SELECT 1 FROM glpi_impactrelations WHERE (itemtype_source = ",
                );
                sql.push_bind(kind.itemtype())
                    .push(" AND items_id_source = ")
                    .push_bind(id)
                    .push(") OR (itemtype_impacted = ")
                    .push_bind(kind.itemtype())
                    .push(" AND items_id_impacted = ")
                    .push_bind(id)
                    .push(") UNION ALL SELECT 1 FROM glpi_impactitems WHERE itemtype = ")
                    .push_bind(kind.itemtype())
                    .push(" AND items_id = ")
                    .push_bind(id);
                ("glpi_impactrelations", sql)
            }
        };
        if exists(pool, sql).await? {
            found.push(name);
        }
    }
    Ok(found)
}

/// Deletes the asset, its history and its linked rows. `false` when it no
/// longer exists.
pub async fn purge(pool: &MySqlPool, kind: AssetKind, id: i32) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let found: Option<i32> = sqlx::query_scalar(&format!(
        "SELECT id FROM {} WHERE id = ? FOR UPDATE",
        kind.table()
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if found.is_none() {
        return Ok(false);
    }
    for link in kind.purged_links() {
        delete_link(&mut tx, kind, id, link).await?;
    }
    // `cleanHistory`.
    sqlx::query("DELETE FROM glpi_logs WHERE itemtype = ? AND items_id = ?")
        .bind(kind.itemtype())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(&format!("DELETE FROM {} WHERE id = ?", kind.table()))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

async fn delete_link(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    id: i32,
    link: Link,
) -> Result<(), sqlx::Error> {
    let mut select = QueryBuilder::new(format!("SELECT id FROM {} WHERE ", link.table));
    push_owner(&mut select, kind, id, link.owner);
    let ids: Vec<i32> = select.build_query_scalar().fetch_all(&mut *conn).await?;
    if ids.is_empty() {
        return Ok(());
    }
    let mut tables: Vec<(&str, Option<&str>)> = Vec::new();
    if let Some(class) = link.history {
        tables.push(("glpi_logs", Some(class)));
    }
    if let Some(class) = link.alerts {
        tables.push(("glpi_alerts", Some(class)));
    }
    tables.push((link.table, None));
    for (table, class) in tables {
        let mut delete = QueryBuilder::new(format!("DELETE FROM {table} WHERE "));
        match class {
            Some(class) => {
                delete
                    .push("itemtype = ")
                    .push_bind(class)
                    .push(" AND items_id IN (");
            }
            None => {
                delete.push("id IN (");
            }
        }
        let mut separated = delete.separated(", ");
        for row in &ids {
            separated.push_bind(*row);
        }
        delete.push(")");
        delete.build().execute(&mut *conn).await?;
    }
    Ok(())
}
