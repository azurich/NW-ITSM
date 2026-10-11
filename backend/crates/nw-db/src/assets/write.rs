//! Writes on the assets, with the history and side effects of the PHP
//! code (`CommonDBTM::add`, `update`, `delete`, `restore`,
//! `Computer::post_updateItem`).
//!
//! Each function runs in one transaction. Rights, entities and the checks
//! for PHP features not ported yet are the caller's job.

use std::collections::{HashMap, HashSet};

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
    pub is_template: bool,
    /// Stored values of the fields of the form (of a template, with its
    /// name).
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
    /// Connected items whose update is refused by a unicity criterion:
    /// they do not follow.
    pub refused_items: &'a HashSet<(AssetKind, i32)>,
    pub infocoms: &'a InfocomSettings,
}

/// What a creation starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin<'a> {
    /// The form of a new item.
    Form,
    /// The form of a new template, its name among the values.
    Template,
    /// The form of an item created from a template (`CommonDBTM::clone`):
    /// the other columns of the template are copied, and its infocom,
    /// contracts and documents cloned (`Clonable::post_clone`).
    FromTemplate {
        id: i32,
        /// `"%1$s (%2$s)"` in the language of the author, for the names in
        /// the history of the cloned links.
        name_id: &'a str,
    },
}

/// Creates an asset (legacy `CommonDBTM::add`), with its infocom when
/// configured. Returns its id, `None` when the template to copy no longer
/// exists.
pub async fn create(
    pool: &MySqlPool,
    kind: AssetKind,
    entity_id: i32,
    values: &[(Field, Value)],
    actor: &str,
    infocom_settings: &InfocomSettings,
    origin: Origin<'_>,
) -> Result<Option<i32>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // The columns of the template not in the form are copied.
    let copied: Vec<String> = match origin {
        Origin::FromTemplate { .. } => {
            let set = [
                "id",
                "entities_id",
                "date_creation",
                "date_mod",
                "is_template",
                "template_name",
            ];
            crate::schema::columns(&mut tx, kind.table())
                .await?
                .into_iter()
                .filter(|column| {
                    !set.contains(&column.as_str())
                        && !values.iter().any(|(field, _)| field.column == column)
                })
                .collect()
        }
        Origin::Form | Origin::Template => Vec::new(),
    };
    let mut sql = QueryBuilder::<MySql>::new(format!("INSERT INTO {} (entities_id", kind.table()));
    for (field, _) in values {
        sql.push(format_args!(", {}", field.column));
    }
    for column in &copied {
        sql.push(format_args!(", `{column}`"));
    }
    if origin == Origin::Template {
        sql.push(", is_template");
    }
    sql.push(", date_creation, date_mod) ");
    sql.push(match origin {
        Origin::FromTemplate { .. } => "SELECT ",
        Origin::Form | Origin::Template => "VALUES (",
    });
    sql.push_bind(entity_id);
    for (field, value) in values {
        sql.push(", ");
        push_value(&mut sql, *field, value);
    }
    for column in &copied {
        sql.push(format_args!(", t.`{column}`"));
    }
    if origin == Origin::Template {
        sql.push(", 1");
    }
    sql.push(", NOW(), NOW()");
    match origin {
        Origin::FromTemplate { id, .. } => {
            sql.push(format_args!(
                " FROM {} t WHERE t.is_template = 1 AND t.id = ",
                kind.table()
            ));
            sql.push_bind(id);
        }
        Origin::Form | Origin::Template => {
            sql.push(")");
        }
    }
    let result = sql.build().execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    let id = i32::try_from(result.last_insert_id())
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
        forwards_entity: kind.forwards_to_infocom(),
    };
    // Not for a clone.
    if kind.has_infocom()
        && infocom_settings.auto_create
        && !matches!(origin, Origin::FromTemplate { .. })
    {
        infocoms::create_for_new_item(&mut tx, infocom_settings, owner, actor).await?;
    }
    // The form always posts a status, even none; not for a template nor a
    // type without status (software).
    if kind.has_infocom() && kind.has_column("states_id") && origin != Origin::Template {
        let states_id = value_of("states_id").map_or(0, Value::id);
        infocoms::fill_status_dates(&mut tx, infocom_settings, owner, states_id, actor).await?;
    }
    if let Origin::FromTemplate {
        id: template_id,
        name_id,
    } = origin
    {
        super::template::clone_links(
            &mut tx,
            kind,
            template_id,
            owner,
            name_id,
            infocom_settings,
            actor,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Some(id))
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
                if effects.refused_items.contains(&(item_kind, item_id)) {
                    continue;
                }
                let mut item_values: Vec<(Field, Value)> = to_items
                    .iter()
                    .filter_map(|column| Some((item_kind.field(column)?, new_value(column)?)))
                    .collect();
                // The business rules of the item's update run on the copied
                // fields; what they set wins (in place, else last).
                for (field, value) in effects
                    .connected_rules
                    .get(&item_kind)
                    .into_iter()
                    .flatten()
                {
                    match item_values
                        .iter_mut()
                        .find(|(existing, _)| existing.column == field.column)
                    {
                        Some(existing) => existing.1 = value.clone(),
                        None => item_values.push((*field, value.clone())),
                    }
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
        forwards_entity: kind.forwards_to_infocom(),
    };
    // Only the classes forwarding their entity (`$forward_entity_to`).
    if changed.contains(&"is_recursive") && kind.forwards_to_infocom() {
        infocoms::forward_entity(conn, owner).await?;
        forward_sharing(conn, kind, current.id, owner.is_recursive).await?;
    }
    if changed.contains(&"states_id") && kind.has_infocom() {
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
    let fields = if kind.has_templates() {
        kind.template_fields()
    } else {
        kind.fields()
    };
    let columns: Vec<String> = fields.iter().map(|field| select_column(*field)).collect();
    let row = sqlx::query(&format!(
        "SELECT id, entities_id, is_deleted, {is_dynamic} AS is_dynamic, \
         {is_template} AS is_template, {} FROM {} WHERE id = ? FOR UPDATE",
        columns.join(", "),
        kind.table(),
        is_dynamic = if kind.has_column("is_dynamic") {
            "is_dynamic"
        } else {
            "0"
        },
        is_template = if kind.has_templates() {
            "is_template"
        } else {
            "0"
        },
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let is_template = row.try_get::<i64, _>("is_template")? != 0;
    let values: Vec<(Field, Value)> = fields
        .into_iter()
        .filter(|field| is_template || field.column != "template_name")
        .map(|field| Ok((field, read_value(&row, field)?)))
        .collect::<Result<_, sqlx::Error>>()?;
    // Only some tables have the column.
    let is_global = values
        .iter()
        .any(|(field, value)| field.column == "is_global" && *value == Value::Bool(true));
    Ok(Some(Current {
        id: row.try_get("id")?,
        entities_id: row.try_get("entities_id")?,
        is_deleted: row.try_get("is_deleted")?,
        is_dynamic: row.try_get::<i64, _>("is_dynamic")? != 0,
        is_global,
        is_template,
        values,
    }))
}

/// A column in a `SELECT`: numbers as text, compared as stored.
fn select_column(field: Field) -> String {
    match field.kind {
        FieldKind::Integer | FieldKind::Decimal | FieldKind::Number => {
            format!("CAST({0} AS CHAR) AS {0}", field.column)
        }
        _ => field.column.to_owned(),
    }
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
        FieldKind::Text | FieldKind::Integer | FieldKind::Decimal | FieldKind::Number => {
            Value::Text(row.try_get(field.column)?)
        }
        FieldKind::Dropdown { .. } | FieldKind::User => row
            .try_get::<Option<i32>, _>(field.column)?
            .map_or(Value::NoId, Value::Id),
        FieldKind::Bool => Value::Bool(row.try_get(field.column)?),
    })
}

/// A value as written in its column: an empty number is `0`, as the PHP
/// connection (without strict SQL mode) stores it.
fn push_value(sql: &mut QueryBuilder<'_, MySql>, field: Field, value: &Value) {
    match value {
        Value::Text(text)
            if field.kind == FieldKind::Number
                && text.as_deref().unwrap_or_default().is_empty() =>
        {
            sql.push_bind(0)
        }
        Value::Text(text) => sql.push_bind(text.clone()),
        Value::Id(id) => sql.push_bind(*id),
        Value::NoId => sql.push_bind(None::<i32>),
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
            field.changes(old, new).then(|| (*field, new, old.clone()))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = QueryBuilder::<MySql>::new(format!("UPDATE {} SET ", kind.table()));
    for (field, new, _) in &changes {
        sql.push(format_args!("{} = ", field.column));
        push_value(&mut sql, *field, new);
        sql.push(", ");
    }
    sql.push("date_mod = NOW() WHERE id = ");
    sql.push_bind(current.id);
    sql.build().execute(&mut *conn).await?;

    for (field, new, old) in changes.iter().filter(|(field, _, _)| field.is_logged()) {
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

/// A value as `Log::constructHistory` writes it: raw for text, numbers and
/// booleans, `"<name> (<id>)"` for dropdowns (users by login, trees by full
/// name).
pub(crate) async fn history_label(
    conn: &mut MySqlConnection,
    kind: FieldKind,
    value: &Value,
) -> Result<String, sqlx::Error> {
    let id = match (kind, value) {
        (
            FieldKind::Text
            | FieldKind::Bool
            | FieldKind::Integer
            | FieldKind::Decimal
            | FieldKind::Number,
            value,
        ) => {
            return Ok(value.raw());
        }
        // `getDropdownName` of NULL, and the NULL id printed empty.
        (_, Value::NoId) => return Ok(format!("{} ()", history::NO_VALUE)),
        (_, value) => value.id(),
    };
    let name = if id == 0 {
        None
    } else {
        let (table, column) = match kind {
            FieldKind::User => ("glpi_users", "name"),
            FieldKind::Dropdown { table, tree: true } => (table, "completename"),
            FieldKind::Dropdown { table, tree: false } => (table, "name"),
            FieldKind::Text
            | FieldKind::Bool
            | FieldKind::Integer
            | FieldKind::Decimal
            | FieldKind::Number => {
                unreachable!("handled above")
            }
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
            FieldKind::Text
            | FieldKind::Bool
            | FieldKind::Integer
            | FieldKind::Decimal
            | FieldKind::Number => continue,
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

/// A non-global item connected to a computer: it follows its changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectedItem {
    pub kind: AssetKind,
    pub id: i32,
    pub entities_id: i32,
}

/// The items following the changes of a computer.
pub async fn connected(
    pool: &MySqlPool,
    computer_id: i32,
) -> Result<Vec<ConnectedItem>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let mut items = Vec::new();
    for (kind, id) in connected_items(&mut conn, computer_id).await? {
        let is_global = if kind.can_be_global() {
            "is_global"
        } else {
            "0"
        };
        let row: Option<(i64, i32)> = sqlx::query_as(&format!(
            "SELECT CAST({is_global} AS SIGNED), entities_id FROM {} WHERE id = ?",
            kind.table()
        ))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((0, entities_id)) = row {
            items.push(ConnectedItem {
                kind,
                id,
                entities_id,
            });
        }
    }
    Ok(items)
}

/// Tables following the entity and sharing of an asset
/// (`$forward_entity_to`) whose rows of this asset are in another entity:
/// copying the entity to them is not ported.
pub async fn entity_mismatches(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    entity: i32,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut found = Vec::new();
    let infocom: &[&str] = if kind.forwards_to_infocom() {
        &["glpi_infocoms"]
    } else {
        &[]
    };
    for &table in kind.forward_entity_tables().iter().chain(infocom) {
        // Rows of a child class point to the asset by its foreign key.
        let owner_column = match table {
            "glpi_computervirtualmachines" => Some("computers_id"),
            "glpi_softwareversions" => Some("softwares_id"),
            _ => None,
        };
        let owner = owner_column.map_or_else(
            || "itemtype = ? AND items_id = ?".to_owned(),
            |column| format!("{column} = ?"),
        );
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {owner} AND entities_id <> ?");
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        if owner_column.is_none() {
            query = query.bind(kind.itemtype());
        }
        if query.bind(id).bind(entity).fetch_one(pool).await? > 0 {
            found.push(table);
        }
    }
    Ok(found)
}

/// Linked items of entities that would no longer see a shared asset
/// (`CommonDBTM::canUnrecurs`, `Printer::canUnrecurs`,
/// `NetworkEquipment::canUnrecurs`); `entities` are the entity of the
/// asset and its parents.
pub async fn unshare_blockers(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
    entities: &[i32],
) -> Result<Vec<&'static str>, sqlx::Error> {
    let not_in = |column: &str| {
        let list = vec!["?"; entities.len()].join(", ");
        format!("{column} NOT IN ({list})")
    };
    let mut found = Vec::new();
    if kind == AssetKind::Computer {
        let checks = [
            (
                "glpi_items_softwareversions",
                format!(
                    "SELECT COUNT(*) FROM glpi_items_softwareversions \
                     WHERE itemtype = 'Computer' AND items_id = ? AND {}",
                    not_in("entities_id")
                ),
            ),
            (
                "glpi_computervirtualmachines",
                format!(
                    "SELECT COUNT(*) FROM glpi_computervirtualmachines WHERE computers_id = ? AND {}",
                    not_in("entities_id")
                ),
            ),
        ];
        for (table, sql) in checks {
            let mut query = sqlx::query_scalar::<_, i64>(&sql).bind(id);
            for entity in entities {
                query = query.bind(*entity);
            }
            if query.fetch_one(pool).await? > 0 {
                found.push(table);
            }
        }
    }
    if kind == AssetKind::Software {
        // `canUnrecurs`: the software upgrading it (`softwares_id`).
        let sql = format!(
            "SELECT COUNT(*) FROM glpi_softwares WHERE softwares_id = ? AND {}",
            not_in("entities_id")
        );
        let mut query = sqlx::query_scalar::<_, i64>(&sql).bind(id);
        for entity in entities {
            query = query.bind(*entity);
        }
        if query.fetch_one(pool).await? > 0 {
            found.push("glpi_softwares");
        }
    }
    if matches!(kind, AssetKind::Printer | AssetKind::NetworkEquipment) {
        // Ports wired to the ports of items of other entities.
        let wired: Vec<(String, i32)> = sqlx::query_as(
            "SELECT other.itemtype, other.items_id FROM glpi_networkports mine \
             INNER JOIN glpi_networkports_networkports wire \
               ON wire.networkports_id_1 = mine.id OR wire.networkports_id_2 = mine.id \
             INNER JOIN glpi_networkports other ON other.id = \
               IF(wire.networkports_id_1 = mine.id, wire.networkports_id_2, wire.networkports_id_1) \
             WHERE mine.itemtype = ? AND mine.items_id = ?",
        )
        .bind(kind.itemtype())
        .bind(id)
        .fetch_all(pool)
        .await?;
        for (itemtype, items_id) in wired {
            let Some(other) = AssetKind::from_itemtype(&itemtype) else {
                continue;
            };
            let entity: Option<i32> = sqlx::query_scalar(&format!(
                "SELECT entities_id FROM {} WHERE id = ?",
                other.table()
            ))
            .bind(items_id)
            .fetch_optional(pool)
            .await?;
            if entity.is_some_and(|entity| !entities.contains(&entity)) {
                found.push("glpi_networkports_networkports");
                break;
            }
        }
    }
    Ok(found)
}

/// Copies the sharing of an asset to the linked items having the flag
/// (`forwardEntityInformations`: no history, a new `date_mod` where the
/// table has one). The entity of the rows is the asset's (checked before).
async fn forward_sharing(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    id: i32,
    is_recursive: bool,
) -> Result<(), sqlx::Error> {
    for (table, has_date_mod) in [
        ("glpi_networkports", true),
        ("glpi_reservationitems", false),
        ("glpi_items_operatingsystems", true),
    ] {
        let date_mod = if has_date_mod {
            ", date_mod = NOW()"
        } else {
            ""
        };
        sqlx::query(&format!(
            "UPDATE {table} SET is_recursive = ?{date_mod} \
             WHERE itemtype = ? AND items_id = ? AND is_recursive <> ?"
        ))
        .bind(is_recursive)
        .bind(kind.itemtype())
        .bind(id)
        .bind(is_recursive)
        .execute(&mut *conn)
        .await?;
    }
    // The versions of a software (`SoftwareVersion`, a child class).
    if kind == AssetKind::Software {
        sqlx::query(
            "UPDATE glpi_softwareversions SET is_recursive = ?, date_mod = NOW() \
             WHERE softwares_id = ? AND is_recursive <> ?",
        )
        .bind(is_recursive)
        .bind(id)
        .bind(is_recursive)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
