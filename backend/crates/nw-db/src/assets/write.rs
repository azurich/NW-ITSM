//! Writes on the assets, with the history and side effects of the PHP
//! code (`CommonDBTM::add`, `update`, `delete`, `restore`,
//! `Computer::post_updateItem`).
//!
//! Each function runs in one transaction. Rights, entities and the checks
//! for PHP features not ported yet are the caller's job.

use std::collections::HashMap;

use nw_domain::{
    asset::{
        AssetKind, DEVICE_FIELDS, DEVICE_ITEMS, DIRECT_CONNECT_KINDS, Field, FieldKind,
        Propagation, Value,
    },
    history::{self, Action, Change, Entry},
};
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder, Row, mysql::MySqlRow};

use crate::infocoms::{self, InfocomSettings, Owner};

/// Stored state of an asset, locked for the transaction.
#[derive(Debug, Clone)]
pub struct Current {
    pub id: i32,
    pub entities_id: i32,
    pub is_deleted: bool,
    pub is_dynamic: bool,
    pub is_global: bool,
    pub values: Vec<(Field, Value)>,
}

impl Current {
    pub fn value(&self, column: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(field, _)| field.column == column)
            .map(|(_, value)| value)
    }
}

/// What an update changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateOutcome {
    /// Changed columns of the asset.
    pub changed: Vec<&'static str>,
    /// Connected items that followed the changes (type, id).
    pub connected_items: Vec<(AssetKind, i32)>,
    /// Components that followed the changes (type, id).
    pub devices: Vec<(&'static str, i32)>,
}

/// What an update writes besides the values of the item.
#[derive(Debug, Clone, Copy)]
pub struct UpdateEffects<'a> {
    /// Fields of a computer copied to its connected items and components.
    pub propagation: Propagation,
    /// Values set by the business rules on the connected items of each
    /// type, run on the copied fields.
    pub connected_rules: &'a HashMap<AssetKind, Vec<(Field, Value)>>,
    pub infocoms: &'a InfocomSettings,
}

/// Creates an asset (legacy `CommonDBTM::add`), with its infocom when
/// configured. Returns its id.
pub async fn create(
    pool: &MySqlPool,
    kind: AssetKind,
    entity_id: i32,
    values: &[(Field, Value)],
    actor: &str,
    infocom_settings: &InfocomSettings,
) -> Result<i32, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut sql = QueryBuilder::<MySql>::new(format!("INSERT INTO {} (entities_id", kind.table()));
    for (field, _) in values {
        sql.push(format_args!(", {}", field.column));
    }
    sql.push(", date_creation, date_mod) VALUES (");
    sql.push_bind(entity_id);
    for (_, value) in values {
        sql.push(", ");
        push_value(&mut sql, value);
    }
    sql.push(", NOW(), NOW())");
    let id = i32::try_from(sql.build().execute(&mut *tx).await?.last_insert_id())
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;

    let entry = Entry::item(kind.itemtype(), id, Action::CreateItem);
    crate::history::record(&mut tx, actor, &entry).await?;

    let value_of = |column: &str| {
        values
            .iter()
            .find(|(field, _)| field.column == column)
            .map(|(_, value)| value)
    };
    let owner = Owner {
        itemtype: kind.itemtype(),
        id,
        entities_id: entity_id,
        is_recursive: value_of("is_recursive") == Some(&Value::Bool(true)),
    };
    if infocom_settings.auto_create {
        infocoms::create_for_new_item(&mut tx, infocom_settings, owner, actor).await?;
    }
    // The form always posts a status, even none.
    let states_id = value_of("states_id").map_or(0, Value::id);
    infocoms::fill_status_dates(&mut tx, infocom_settings, owner, states_id, actor).await?;
    tx.commit().await?;
    Ok(id)
}

/// Updates an asset (legacy `CommonDBTM::update`) and, for computers,
/// copies the changes to the connected items and components as configured.
/// Returns `None` when the asset does not exist.
pub async fn update(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    values: &[(Field, Value)],
    actor: &str,
    effects: UpdateEffects<'_>,
) -> Result<Option<UpdateOutcome>, sqlx::Error> {
    let propagation = effects.propagation;
    let mut tx = pool.begin().await?;
    let Some(current) = lock(&mut tx, kind, id).await? else {
        return Ok(None);
    };
    let mut outcome = UpdateOutcome {
        changed: apply_changes(&mut tx, kind, &current, values, actor).await?,
        ..UpdateOutcome::default()
    };
    after_update(
        &mut tx,
        kind,
        &current,
        values,
        &outcome.changed,
        effects.infocoms,
        actor,
    )
    .await?;

    if kind == AssetKind::Computer && !outcome.changed.is_empty() {
        let new_value = |column: &str| {
            values
                .iter()
                .find(|(field, _)| field.column == column)
                .map(|(_, value)| value.clone())
        };

        let to_items = propagation.to_connected_items(&outcome.changed);
        if !to_items.is_empty() {
            for (item_kind, item_id) in connected_items(&mut tx, id).await? {
                let Some(item) = lock(&mut tx, item_kind, item_id).await? else {
                    continue;
                };
                if item.is_global {
                    continue;
                }
                let mut item_values: Vec<(Field, Value)> = to_items
                    .iter()
                    .filter_map(|column| Some((item_kind.field(column)?, new_value(column)?)))
                    .collect();
                // The business rules of the item's update run on the copied
                // fields; what they set wins.
                for (field, value) in effects
                    .connected_rules
                    .get(&item_kind)
                    .into_iter()
                    .flatten()
                {
                    item_values.retain(|(existing, _)| existing.column != field.column);
                    item_values.push((*field, value.clone()));
                }
                let changed = apply_changes(&mut tx, item_kind, &item, &item_values, actor).await?;
                after_update(
                    &mut tx,
                    item_kind,
                    &item,
                    &item_values,
                    &changed,
                    effects.infocoms,
                    actor,
                )
                .await?;
                if !changed.is_empty() {
                    outcome.connected_items.push((item_kind, item_id));
                }
            }
        }

        let to_devices = propagation.to_devices(&outcome.changed);
        if !to_devices.is_empty() {
            for (device_itemtype, table) in DEVICE_ITEMS {
                let rows = sqlx::query(&format!(
                    "SELECT id, locations_id, states_id FROM {table} \
                     WHERE itemtype = 'Computer' AND items_id = ? AND is_deleted = 0 FOR UPDATE"
                ))
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
                for row in rows {
                    let device_id: i32 = row.try_get("id")?;
                    let mut changed = false;
                    for column in &to_devices {
                        let Some(field) =
                            DEVICE_FIELDS.iter().find(|field| field.column == *column)
                        else {
                            continue;
                        };
                        let old: i32 = row.try_get(*column)?;
                        let new = new_value(column).map(|value| value.id()).unwrap_or(old);
                        if old == new {
                            continue;
                        }
                        sqlx::query(&format!("UPDATE {table} SET {column} = ? WHERE id = ?"))
                            .bind(new)
                            .bind(device_id)
                            .execute(&mut *tx)
                            .await?;
                        let change = Change::new(
                            field.search_option,
                            &history_label(&mut tx, field.kind, &Value::Id(old)).await?,
                            &history_label(&mut tx, field.kind, &Value::Id(new)).await?,
                        );
                        let own = Entry::field(device_itemtype, device_id, change);
                        crate::history::record(&mut tx, actor, &own).await?;
                        let link =
                            Entry::device_update("Computer", id, device_itemtype, column, old, new);
                        crate::history::record(&mut tx, actor, &link).await?;
                        changed = true;
                    }
                    if changed {
                        outcome.devices.push((device_itemtype, device_id));
                    }
                }
            }
        }
    }

    tx.commit().await?;
    Ok(Some(outcome))
}

/// What follows an update of the item itself (`CommonDBTM::update`): its
/// infocom gets the new sharing (`forwardEntityInformations`) and the
/// dates of its new status (`Infocom::manageDateOnStatusChange`).
async fn after_update(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    current: &Current,
    values: &[(Field, Value)],
    changed: &[&str],
    infocom_settings: &InfocomSettings,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let value_of = |column: &str| {
        values
            .iter()
            .find(|(field, _)| field.column == column)
            .map(|(_, value)| value)
            .or_else(|| current.value(column))
    };
    let owner = Owner {
        itemtype: kind.itemtype(),
        id: current.id,
        entities_id: current.entities_id,
        is_recursive: value_of("is_recursive") == Some(&Value::Bool(true)),
    };
    if changed.contains(&"is_recursive") {
        infocoms::forward_entity(conn, owner).await?;
    }
    if changed.contains(&"states_id") {
        let states_id = value_of("states_id").map_or(0, Value::id);
        infocoms::fill_status_dates(conn, infocom_settings, owner, states_id, actor).await?;
    }
    Ok(())
}

/// Moves an asset to the trash or restores it (legacy `CommonDBTM::delete`
/// without purge, and `restore`). Returns `None` when the asset does not
/// exist and `Some(false)` when it already was in that state.
pub async fn set_trashed(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    trashed: bool,
    actor: &str,
) -> Result<Option<bool>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock(&mut tx, kind, id).await? else {
        return Ok(None);
    };
    if current.is_deleted == trashed {
        return Ok(Some(false));
    }
    sqlx::query(&format!(
        "UPDATE {} SET is_deleted = ?, date_mod = NOW() WHERE id = ?",
        kind.table()
    ))
    .bind(trashed)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Items from an inventory tool are "locked" rather than deleted.
    let action = match (trashed, current.is_dynamic) {
        (true, false) => Action::DeleteItem,
        (true, true) => Action::LockItem,
        (false, false) => Action::RestoreItem,
        (false, true) => Action::UnlockItem,
    };
    crate::history::record(&mut tx, actor, &Entry::item(kind.itemtype(), id, action)).await?;

    if kind == AssetKind::Computer {
        // Legacy Item_SoftwareVersion::updateDatasForItem.
        sqlx::query(
            "UPDATE glpi_items_softwareversions SET is_deleted_item = ?, is_template_item = 0 \
             WHERE itemtype = 'Computer' AND items_id = ?",
        )
        .bind(trashed)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Some(true))
}

/// Reads and locks an asset (templates included).
async fn lock(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    id: i32,
) -> Result<Option<Current>, sqlx::Error> {
    let fields = kind.fields();
    let columns: Vec<&str> = fields.iter().map(|field| field.column).collect();
    // A number for every type: only some tables have the column.
    let is_global = if kind.can_be_global() {
        "CAST(is_global AS SIGNED) AS is_global"
    } else {
        "CAST(0 AS SIGNED) AS is_global"
    };
    let row = sqlx::query(&format!(
        "SELECT id, entities_id, is_deleted, is_dynamic, {is_global}, {} FROM {} WHERE id = ? FOR UPDATE",
        columns.join(", "),
        kind.table()
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let values = fields
        .into_iter()
        .map(|field| Ok((field, read_value(&row, field)?)))
        .collect::<Result<_, sqlx::Error>>()?;
    Ok(Some(Current {
        id: row.try_get("id")?,
        entities_id: row.try_get("entities_id")?,
        is_deleted: row.try_get("is_deleted")?,
        is_dynamic: row.try_get("is_dynamic")?,
        is_global: row.try_get::<i64, _>("is_global")? != 0,
        values,
    }))
}

/// Reads and locks an asset, outside of a write.
pub async fn current(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
) -> Result<Option<Current>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let current = lock(&mut tx, kind, id).await?;
    tx.rollback().await?;
    Ok(current)
}

fn read_value(row: &MySqlRow, field: Field) -> Result<Value, sqlx::Error> {
    Ok(match field.kind {
        FieldKind::Text => Value::Text(row.try_get(field.column)?),
        FieldKind::Dropdown { .. } | FieldKind::User => Value::Id(row.try_get(field.column)?),
        FieldKind::Bool => Value::Bool(row.try_get(field.column)?),
    })
}

fn push_value(sql: &mut QueryBuilder<'_, MySql>, value: &Value) {
    match value {
        Value::Text(text) => sql.push_bind(text.clone()),
        Value::Id(id) => sql.push_bind(*id),
        Value::Bool(flag) => sql.push_bind(*flag),
    };
}

/// Stores the values that differ, with `date_mod` and one history entry per
/// changed field. Returns the changed columns.
async fn apply_changes(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    current: &Current,
    values: &[(Field, Value)],
    actor: &str,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let changes: Vec<(Field, &Value, Value)> = values
        .iter()
        .filter_map(|(field, new)| {
            let old = current.value(field.column)?;
            new.differs_from(old).then(|| (*field, new, old.clone()))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = QueryBuilder::<MySql>::new(format!("UPDATE {} SET ", kind.table()));
    for (field, new, _) in &changes {
        sql.push(format_args!("{} = ", field.column));
        push_value(&mut sql, new);
        sql.push(", ");
    }
    sql.push("date_mod = NOW() WHERE id = ");
    sql.push_bind(current.id);
    sql.build().execute(&mut *conn).await?;

    for (field, new, old) in &changes {
        let change = Change::new(
            field.search_option,
            &history_label(conn, field.kind, old).await?,
            &history_label(conn, field.kind, new).await?,
        );
        let entry = Entry::field(kind.itemtype(), current.id, change);
        crate::history::record(conn, actor, &entry).await?;
    }
    Ok(changes.iter().map(|(field, _, _)| field.column).collect())
}

/// A value as `Log::constructHistory` writes it: raw for text and booleans,
/// `"<name> (<id>)"` for dropdowns (users by login, trees by full name).
async fn history_label(
    conn: &mut MySqlConnection,
    kind: FieldKind,
    value: &Value,
) -> Result<String, sqlx::Error> {
    let id = match (kind, value) {
        (FieldKind::Text | FieldKind::Bool, value) => return Ok(value.raw()),
        (_, value) => value.id(),
    };
    let name = if id == 0 {
        None
    } else {
        let (table, column) = match kind {
            FieldKind::User => ("glpi_users", "name"),
            FieldKind::Dropdown { table, tree: true } => (table, "completename"),
            FieldKind::Dropdown { table, tree: false } => (table, "name"),
            FieldKind::Text | FieldKind::Bool => unreachable!("handled above"),
        };
        let name: Option<Option<String>> =
            sqlx::query_scalar(&format!("SELECT {column} FROM {table} WHERE id = ?"))
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        name.flatten().map(|name| match kind {
            FieldKind::Dropdown { tree: true, .. } => history::tree_name(&name),
            _ => name,
        })
    };
    Ok(history::dropdown_value(name.as_deref(), id))
}

/// Items connected to a computer that follow its changes.
async fn connected_items(
    conn: &mut MySqlConnection,
    computer_id: i32,
) -> Result<Vec<(AssetKind, i32)>, sqlx::Error> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT itemtype, items_id FROM glpi_computers_items \
         WHERE computers_id = ? AND is_deleted = 0 ORDER BY id",
    )
    .bind(computer_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(itemtype, id)| {
            let kind = AssetKind::from_itemtype(&itemtype)?;
            DIRECT_CONNECT_KINDS.contains(&kind).then_some((kind, id))
        })
        .collect())
}

/// Whether every given id exists in its dropdown table (`0` always does).
pub async fn references_exist(
    pool: &MySqlPool,
    values: &[(Field, Value)],
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut missing = Vec::new();
    for (field, value) in values {
        let table = match field.kind {
            FieldKind::User => "glpi_users",
            FieldKind::Dropdown { table, .. } => table,
            FieldKind::Text | FieldKind::Bool => continue,
        };
        let id = value.id();
        if id == 0 {
            continue;
        }
        let found: Option<i32> =
            sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE id = ?"))
                .bind(id)
                .fetch_optional(pool)
                .await?;
        if found.is_none() {
            missing.push(field.column);
        }
    }
    Ok(missing)
}

/// Types of the non-global items connected to a computer: those that follow
/// its changes (and run their own business rules).
pub async fn connected_kinds(
    pool: &MySqlPool,
    computer_id: i32,
) -> Result<Vec<AssetKind>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let mut kinds = Vec::new();
    for (kind, id) in connected_items(&mut conn, computer_id).await? {
        if kinds.contains(&kind) {
            continue;
        }
        let is_global = if kind.can_be_global() {
            "is_global"
        } else {
            "0"
        };
        let global: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT CAST({is_global} AS SIGNED) FROM {} WHERE id = ?",
            kind.table()
        ))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        if global == Some(0) {
            kinds.push(kind);
        }
    }
    Ok(kinds)
}

/// Tables of the items that follow the entity and sharing of an asset
/// (`$forward_entity_to`), infocoms excepted, holding rows of this asset.
pub async fn forwarded_items(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut found = Vec::new();
    for &table in kind.forward_entity_tables() {
        let condition = if table == "glpi_computervirtualmachines" {
            "computers_id = ?"
        } else {
            "itemtype = ? AND items_id = ?"
        };
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {condition}");
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        if table != "glpi_computervirtualmachines" {
            query = query.bind(kind.itemtype());
        }
        if query.bind(id).fetch_one(pool).await? > 0 {
            found.push(table);
        }
    }
    Ok(found)
}
