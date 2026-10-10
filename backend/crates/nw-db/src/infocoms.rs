//! Financial information of the assets (`glpi_infocoms`), written with the
//! items like the PHP code: created with the item when configured
//! (`auto_create_infocoms`), dates filled when the item gets a status
//! (`Infocom::manageDateOnStatusChange`).

use std::collections::HashMap;

use nw_domain::{
    entity::{EntityTree, used_config},
    history::{Action, Change, Entry},
    infocom::{
        ALERT_END, AUTO_DATES, AutoDate, DEFAULT_ALERT_SETTING, UNDEFINED_SETTING, auto_create,
        date_update, status_dates,
    },
};
use sqlx::{MySqlConnection, MySqlPool, Row};

/// Settings deciding what the writes do to the infocoms.
#[derive(Debug, Clone, Default)]
pub struct InfocomSettings {
    /// `auto_create_infocoms`.
    pub auto_create: bool,
    /// Used values of the `autofill_*` and `default_infocom_alert` entity
    /// settings (inheritance resolved), by entity.
    used: HashMap<i32, HashMap<&'static str, String>>,
}

impl InfocomSettings {
    /// Dates filled when an item of the entity gets the status.
    pub fn status_dates(&self, entity: i32, states_id: i32) -> Vec<AutoDate> {
        let used = self.used.get(&entity);
        status_dates(
            |setting| {
                used.and_then(|values| values.get(setting))
                    .map(String::as_str)
            },
            states_id,
        )
    }

    /// Whether a status fills a date somewhere.
    pub fn any_status_date(&self) -> bool {
        self.used.values().any(|values| {
            AUTO_DATES.iter().any(|date| {
                values
                    .get(date.setting)
                    .is_some_and(|value| value.contains("STATUS_"))
            })
        })
    }

    /// Alert of a new infocom (`Entity::getUsedConfig`, -2 when undefined).
    fn default_alert(&self, entity: i32) -> i32 {
        self.used
            .get(&entity)
            .and_then(|values| values.get(DEFAULT_ALERT_SETTING))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(-2)
    }
}

/// Loads the settings.
pub async fn settings(pool: &MySqlPool) -> Result<InfocomSettings, sqlx::Error> {
    let auto = crate::config::core_values(pool, &["auto_create_infocoms"]).await?;
    let mut columns: Vec<&'static str> = AUTO_DATES.iter().map(|date| date.setting).collect();
    columns.push(DEFAULT_ALERT_SETTING);
    let select: Vec<String> = columns
        .iter()
        .map(|column| format!("CAST({column} AS CHAR) AS {column}"))
        .collect();
    let rows = sqlx::query(&format!(
        "SELECT id, entities_id, {} FROM glpi_entities",
        select.join(", ")
    ))
    .fetch_all(pool)
    .await?;

    let mut raw: HashMap<i32, HashMap<&'static str, Option<String>>> = HashMap::new();
    let mut parents = Vec::new();
    for row in rows {
        let id: i32 = row.try_get("id")?;
        parents.push((id, row.try_get::<i32, _>("entities_id")?));
        let values = columns
            .iter()
            .map(|column| Ok((*column, row.try_get::<Option<String>, _>(*column)?)))
            .collect::<Result<_, sqlx::Error>>()?;
        raw.insert(id, values);
    }
    let tree = EntityTree::new(parents);
    let used = raw
        .keys()
        .map(|&entity| {
            let values = columns
                .iter()
                .map(|&column| {
                    let value = used_config(&tree, entity, |id| {
                        raw.get(&id)
                            .and_then(|values| values.get(column)?.as_deref())
                    });
                    (column, value.unwrap_or(UNDEFINED_SETTING).to_owned())
                })
                .collect();
            (entity, values)
        })
        .collect();
    Ok(InfocomSettings {
        auto_create: auto_create(auto.get("auto_create_infocoms").map(String::as_str)),
        used,
    })
}

/// The asset an infocom belongs to.
#[derive(Debug, Clone, Copy)]
pub struct Owner<'a> {
    pub itemtype: &'a str,
    pub id: i32,
    pub entities_id: i32,
    pub is_recursive: bool,
}

/// Creates the infocom of a new item (`auto_create_infocoms`).
pub async fn create_for_new_item(
    conn: &mut MySqlConnection,
    settings: &InfocomSettings,
    owner: Owner<'_>,
    actor: &str,
) -> Result<(), sqlx::Error> {
    if find(conn, owner).await?.is_none() {
        add(conn, settings, owner, &[], "", actor).await?;
    }
    Ok(())
}

/// Fills the dates set by the status of the item (`manageDateOnStatusChange`):
/// adds the infocom if needed, else updates it.
pub async fn fill_status_dates(
    conn: &mut MySqlConnection,
    settings: &InfocomSettings,
    owner: Owner<'_>,
    states_id: i32,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let dates = settings.status_dates(owner.entities_id, states_id);
    if dates.is_empty() {
        return Ok(());
    }
    let now: String = sqlx::query_scalar("SELECT CAST(NOW() AS CHAR)")
        .fetch_one(&mut *conn)
        .await?;
    let Some(infocom_id) = find(conn, owner).await? else {
        return add(conn, settings, owner, &dates, &now, actor).await;
    };

    let mut stored = Vec::new();
    for date in &dates {
        let value: Option<String> = sqlx::query_scalar(&format!(
            "SELECT CAST({} AS CHAR) FROM glpi_infocoms WHERE id = ? FOR UPDATE",
            date.field
        ))
        .bind(infocom_id)
        .fetch_one(&mut *conn)
        .await?;
        stored.push(value);
    }
    let updates: Vec<_> = dates
        .iter()
        .zip(&stored)
        .map(|(date, old)| {
            (
                *date,
                old.as_deref(),
                date_update(*date, old.as_deref(), &now),
            )
        })
        .filter(|(_, _, update)| update.changed)
        .collect();
    if updates.is_empty() {
        return Ok(());
    }

    // `Infocom::pre_updateInDB`: a later end of warranty clears its alert.
    let warranty_later = updates.iter().any(|(date, old, _)| {
        date.field == "warranty_date" && old.is_some_and(|old| old < now.as_str())
    });
    if warranty_later {
        sqlx::query(
            "DELETE FROM glpi_alerts WHERE itemtype = 'Infocom' AND items_id = ? AND type = ?",
        )
        .bind(infocom_id)
        .bind(ALERT_END)
        .execute(&mut *conn)
        .await?;
    }

    let assignments: Vec<String> = updates
        .iter()
        .map(|(date, _, _)| format!("{} = ?", date.field))
        .collect();
    let sql = format!(
        "UPDATE glpi_infocoms SET {}, date_mod = NOW() WHERE id = ?",
        assignments.join(", ")
    );
    let mut query = sqlx::query(&sql);
    for (date, _, _) in &updates {
        query = query.bind(stored_value(*date, &now));
    }
    query.bind(infocom_id).execute(&mut *conn).await?;

    // Logged on the item, with the time even for a day.
    for (date, old, update) in &updates {
        if update.logged {
            let change = Change::new(date.search_option, old.unwrap_or_default(), &now);
            let entry = Entry::field(owner.itemtype, owner.id, change);
            crate::history::record(conn, actor, &entry).await?;
        }
    }
    Ok(())
}

/// Copies the entity and sharing of an item to its infocom, without
/// history (`CommonDBTM::forwardEntityInformations`).
pub async fn forward_entity(
    conn: &mut MySqlConnection,
    owner: Owner<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE glpi_infocoms SET entities_id = ?, is_recursive = ?, date_mod = NOW() \
         WHERE itemtype = ? AND items_id = ? AND (entities_id <> ? OR is_recursive <> ?)",
    )
    .bind(owner.entities_id)
    .bind(owner.is_recursive)
    .bind(owner.itemtype)
    .bind(owner.id)
    .bind(owner.entities_id)
    .bind(owner.is_recursive)
    .execute(conn)
    .await?;
    Ok(())
}

async fn find(conn: &mut MySqlConnection, owner: Owner<'_>) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM glpi_infocoms WHERE itemtype = ? AND items_id = ? ORDER BY id LIMIT 1 FOR UPDATE")
        .bind(owner.itemtype)
        .bind(owner.id)
        .fetch_optional(conn)
        .await
}

/// `Infocom::add`: entity and sharing of the item, default alert of the
/// entity, and the creation in the history of the infocom.
async fn add(
    conn: &mut MySqlConnection,
    settings: &InfocomSettings,
    owner: Owner<'_>,
    dates: &[AutoDate],
    now: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let mut columns = vec![
        "itemtype",
        "items_id",
        "entities_id",
        "is_recursive",
        "alert",
    ];
    columns.extend(dates.iter().map(|date| date.field));
    let placeholders = vec!["?"; columns.len()].join(", ");
    let sql = format!(
        "INSERT INTO glpi_infocoms ({}, date_creation, date_mod) VALUES ({placeholders}, NOW(), NOW())",
        columns.join(", ")
    );
    let mut query = sqlx::query(&sql)
        .bind(owner.itemtype)
        .bind(owner.id)
        .bind(owner.entities_id)
        .bind(owner.is_recursive)
        .bind(settings.default_alert(owner.entities_id));
    for date in dates {
        query = query.bind(stored_value(*date, now));
    }
    let id = i32::try_from(query.execute(&mut *conn).await?.last_insert_id())
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    crate::history::record(conn, actor, &Entry::item("Infocom", id, Action::CreateItem)).await
}

/// `glpi_currenttime` as stored by the column.
fn stored_value(date: AutoDate, now: &str) -> &str {
    if date.with_time {
        now
    } else {
        now.get(..10).unwrap_or(now)
    }
}
