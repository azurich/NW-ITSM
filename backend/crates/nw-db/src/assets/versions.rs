//! Versions of a software (`glpi_softwareversions`): their own history, and
//! the history of their software (`CommonDBChild`).

use nw_domain::{
    asset::{FieldKind, Value},
    history::{Action, Change, Entry},
    software::{VERSION_FIELDS, VERSION_ITEMTYPE, version_field},
};
use sqlx::{FromRow, MySql, MySqlConnection, MySqlPool, QueryBuilder};

use super::write::history_label;

/// A version, with the names of its references.
#[derive(Debug, Clone, FromRow)]
pub struct VersionRow {
    pub id: i32,
    pub softwares_id: i32,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub entities_id: i32,
    pub entity: Option<String>,
    pub is_recursive: bool,
    pub operatingsystems_id: i32,
    pub operating_system: Option<String>,
    pub states_id: i32,
    pub state: Option<String>,
    /// Installations outside the trash.
    pub installations: i64,
}

const SELECT: &str = "SELECT v.id, v.softwares_id, v.name, v.comment, v.entities_id, \
     e.completename AS entity, v.is_recursive, v.operatingsystems_id, os.name AS operating_system, \
     v.states_id, s.completename AS state, \
     (SELECT COUNT(*) FROM glpi_items_softwareversions i \
      WHERE i.softwareversions_id = v.id AND i.is_deleted = 0) AS installations \
     FROM glpi_softwareversions v \
     LEFT JOIN glpi_entities e ON e.id = v.entities_id \
     LEFT JOIN glpi_operatingsystems os ON os.id = v.operatingsystems_id \
     LEFT JOIN glpi_states s ON s.id = v.states_id";

/// The versions of a software, by name.
pub async fn list(pool: &MySqlPool, software_id: i32) -> Result<Vec<VersionRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE v.softwares_id = ? ORDER BY v.name, v.id"
    ))
    .bind(software_id)
    .fetch_all(pool)
    .await
}

/// A version of a software.
pub async fn get(
    pool: &MySqlPool,
    software_id: i32,
    id: i32,
) -> Result<Option<VersionRow>, sqlx::Error> {
    sqlx::query_as(&format!("{SELECT} WHERE v.softwares_id = ? AND v.id = ?"))
        .bind(software_id)
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// The software of the versions, as their history names it.
#[derive(Debug, Clone)]
pub struct Software {
    pub id: i32,
    pub name: Option<String>,
    pub entities_id: i32,
    pub is_recursive: bool,
}

/// Names a version in the history of its software.
pub type NameVersion<'a> = &'a (dyn Fn(Option<&str>, Option<&str>, i32) -> String + Sync);

/// Adds a version to a software, in its entity: its name in the history of
/// the software (`HISTORY_ADD_SUBITEM`), its creation in its own history.
pub async fn create(
    pool: &MySqlPool,
    software: &Software,
    values: &[(&'static str, Value)],
    actor: &str,
    name_version: NameVersion<'_>,
) -> Result<i32, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let value = |column: &str| {
        values
            .iter()
            .find(|(name, _)| *name == column)
            .map(|(_, v)| v)
    };
    let mut sql = QueryBuilder::<MySql>::new(
        "INSERT INTO glpi_softwareversions (softwares_id, entities_id, is_recursive, name, comment, \
         operatingsystems_id, states_id, date_mod, date_creation) VALUES (",
    );
    let text = |column: &str| match value(column) {
        Some(Value::Text(text)) => text.clone(),
        _ => Some(String::new()),
    };
    let id_of = |column: &str| value(column).map_or(0, Value::id);
    let mut separated = sql.separated(", ");
    separated.push_bind(software.id);
    separated.push_bind(software.entities_id);
    separated.push_bind(match value("is_recursive") {
        Some(Value::Bool(flag)) => *flag,
        _ => software.is_recursive,
    });
    separated.push_bind(text("name"));
    separated.push_bind(text("comment"));
    separated.push_bind(id_of("operatingsystems_id"));
    separated.push_bind(id_of("states_id"));
    sql.push(", NOW(), NOW())");
    let result = sql.build().execute(&mut *tx).await?;
    let id = i32::try_from(result.last_insert_id())
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;

    // The software first (`post_addItem`), then the version.
    let name = text("name");
    let entry = Entry::subitem(
        "Software",
        software.id,
        VERSION_ITEMTYPE,
        Action::AddSubitem,
        "",
        &name_version(software.name.as_deref(), name.as_deref(), id),
    );
    crate::history::record(&mut tx, actor, &entry).await?;
    crate::history::record(
        &mut tx,
        actor,
        &Entry::item(VERSION_ITEMTYPE, id, Action::CreateItem),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Changes a version, in the order of its form: its own history for the
/// fields having a search option, then every change, raw, in the history
/// of its software (`HISTORY_UPDATE_SUBITEM`). `None` when it no longer
/// exists; the changed columns otherwise.
pub async fn update(
    pool: &MySqlPool,
    software: &Software,
    id: i32,
    values: &[(&'static str, Value)],
    actor: &str,
) -> Result<Option<Vec<&'static str>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock(&mut tx, software.id, id).await? else {
        return Ok(None);
    };
    let mut changes = Vec::new();
    for field in VERSION_FIELDS {
        let Some((_, new)) = values.iter().find(|(column, _)| *column == field.column) else {
            continue;
        };
        let Some((_, old)) = current.iter().find(|(column, _)| *column == field.column) else {
            continue;
        };
        if field.changes(old, new) {
            changes.push((field, old.clone(), new.clone()));
        }
    }
    if changes.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut sql = QueryBuilder::<MySql>::new("UPDATE glpi_softwareversions SET ");
    for (field, _, new) in &changes {
        sql.push(format_args!("{} = ", field.column));
        match new {
            Value::Text(text) => sql.push_bind(text.clone()),
            Value::Bool(flag) => sql.push_bind(*flag),
            other => sql.push_bind(other.id()),
        };
        sql.push(", ");
    }
    sql.push("date_mod = NOW() WHERE id = ").push_bind(id);
    sql.build().execute(&mut *tx).await?;

    for (field, old, new) in changes.iter().filter(|(field, _, _)| field.is_logged()) {
        let change = Change::new(
            field.search_option,
            &history_label(&mut tx, field.kind, old).await?,
            &history_label(&mut tx, field.kind, new).await?,
        );
        crate::history::record(&mut tx, actor, &Entry::field(VERSION_ITEMTYPE, id, change)).await?;
    }
    for (_, old, new) in &changes {
        let entry = Entry::subitem(
            "Software",
            software.id,
            VERSION_ITEMTYPE,
            Action::UpdateSubitem,
            &old.raw(),
            &new.raw(),
        );
        crate::history::record(&mut tx, actor, &entry).await?;
    }
    tx.commit().await?;
    Ok(Some(
        changes.iter().map(|(field, _, _)| field.column).collect(),
    ))
}

/// Sharing, comment, name, operating system and status of a version.
type Stored = (bool, Option<String>, Option<String>, i32, i32);

/// The stored values of the fields of a version, locked.
async fn lock(
    conn: &mut MySqlConnection,
    software_id: i32,
    id: i32,
) -> Result<Option<Vec<(&'static str, Value)>>, sqlx::Error> {
    let row: Option<Stored> = sqlx::query_as(
        "SELECT is_recursive, comment, name, operatingsystems_id, states_id \
         FROM glpi_softwareversions WHERE softwares_id = ? AND id = ? FOR UPDATE",
    )
    .bind(software_id)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|(is_recursive, comment, name, os, state)| {
        vec![
            ("is_recursive", Value::Bool(is_recursive)),
            ("comment", Value::Text(comment)),
            ("name", Value::Text(name)),
            ("operatingsystems_id", Value::Id(os)),
            ("states_id", Value::Id(state)),
        ]
    }))
}

/// Licenses bought or used with the version: purging it updates them like
/// a form (`cleanRelationData`, with their history), which is not ported.
pub async fn purge_blockers(pool: &MySqlPool, id: i32) -> Result<Vec<&'static str>, sqlx::Error> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM glpi_softwarelicenses \
         WHERE softwareversions_id_buy = ? OR softwareversions_id_use = ? LIMIT 1",
    )
    .bind(id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(if found.is_some() {
        vec!["glpi_softwarelicenses"]
    } else {
        Vec::new()
    })
}

/// Deletes a version for good: its installations, without history
/// (`cleanDBonPurge`), its history, and its name in the history of its
/// software (`HISTORY_DELETE_SUBITEM`). `false` when it no longer exists.
pub async fn purge(
    pool: &MySqlPool,
    software: &Software,
    id: i32,
    actor: &str,
    name_version: NameVersion<'_>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let name: Option<Option<String>> = sqlx::query_scalar(
        "SELECT name FROM glpi_softwareversions WHERE softwares_id = ? AND id = ? FOR UPDATE",
    )
    .bind(software.id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(name) = name else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM glpi_items_softwareversions WHERE softwareversions_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM glpi_logs WHERE itemtype = ? AND items_id = ?")
        .bind(VERSION_ITEMTYPE)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM glpi_softwareversions WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let entry = Entry::subitem(
        "Software",
        software.id,
        VERSION_ITEMTYPE,
        Action::DeleteSubitem,
        &name_version(software.name.as_deref(), name.as_deref(), id),
        "",
    );
    crate::history::record(&mut tx, actor, &entry).await?;
    tx.commit().await?;
    Ok(true)
}

/// Whether the referenced rows exist (an operating system, a status).
pub async fn references_exist(
    pool: &MySqlPool,
    values: &[(&'static str, Value)],
) -> Result<bool, sqlx::Error> {
    for (column, value) in values {
        let Some(field) = version_field(column) else {
            continue;
        };
        let FieldKind::Dropdown { table, .. } = field.kind else {
            continue;
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
            return Ok(false);
        }
    }
    Ok(true)
}
