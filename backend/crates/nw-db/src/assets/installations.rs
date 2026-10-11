//! Software installed on the assets (`glpi_items_softwareversions`,
//! `Item_SoftwareVersion`): a relation between an asset and a version of a
//! software, noted in the history of both (`CommonDBRelation`).

use chrono::NaiveDate;
use nw_domain::{
    asset::AssetKind,
    entity::EntityScope,
    history::{Action, Entry},
    software::VERSION_ITEMTYPE,
};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use crate::scope::restrict_entities;

/// A software installed on an asset, as its software tab lists it.
#[derive(Debug, Clone, FromRow)]
pub struct InstallationRow {
    pub id: i32,
    pub softwareversions_id: i32,
    pub version: Option<String>,
    pub version_state: Option<String>,
    pub softwares_id: Option<i32>,
    pub software: Option<String>,
    pub softwarecategories_id: Option<i32>,
    pub category: Option<String>,
    /// Licenses of the software are valid.
    pub is_valid: Option<bool>,
    pub date_install: Option<NaiveDate>,
    pub is_dynamic: bool,
}

/// The installations of an asset outside the trash, of the software
/// visible to the session (`Item_SoftwareVersion::getFromItem`), by
/// software and version.
pub async fn list(
    pool: &MySqlPool,
    kind: AssetKind,
    asset_id: i32,
    scope: &EntityScope,
) -> Result<Vec<InstallationRow>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT i.id, i.softwareversions_id, v.name AS version, st.completename AS version_state, \
         v.softwares_id, s.name AS software, s.softwarecategories_id, c.completename AS category, \
         s.is_valid, i.date_install, i.is_dynamic \
         FROM glpi_items_softwareversions i \
         LEFT JOIN glpi_softwareversions v ON v.id = i.softwareversions_id \
         LEFT JOIN glpi_states st ON st.id = v.states_id \
         LEFT JOIN glpi_softwares s ON s.id = v.softwares_id \
         LEFT JOIN glpi_softwarecategories c ON c.id = s.softwarecategories_id \
         WHERE i.items_id = ",
    );
    query
        .push_bind(asset_id)
        .push(" AND i.itemtype = ")
        .push_bind(kind.itemtype())
        .push(" AND i.is_deleted = 0");
    restrict_entities(&mut query, "s", scope);
    query.push(" ORDER BY s.name, v.name, i.id");
    query.build_query_as().fetch_all(pool).await
}

/// A version to install, with its software.
#[derive(Debug, Clone, FromRow)]
pub struct Version {
    pub id: i32,
    pub name: Option<String>,
    pub entities_id: i32,
    pub is_recursive: bool,
    pub softwares_id: i32,
    pub software: Option<String>,
    pub software_entities_id: i32,
    pub software_is_recursive: bool,
}

/// A version and its software; `None` when one of them does not exist.
pub async fn version(pool: &MySqlPool, id: i32) -> Result<Option<Version>, sqlx::Error> {
    sqlx::query_as(
        "SELECT v.id, v.name, v.entities_id, v.is_recursive, v.softwares_id, s.name AS software, \
         s.entities_id AS software_entities_id, s.is_recursive AS software_is_recursive \
         FROM glpi_softwareversions v JOIN glpi_softwares s ON s.id = v.softwares_id \
         WHERE v.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// The asset a version is installed on.
#[derive(Debug, Clone)]
pub struct Asset {
    pub kind: AssetKind,
    pub id: i32,
    pub is_deleted: bool,
    pub is_template: bool,
}

/// Names of the two sides in the history of the other one.
#[derive(Debug, Clone)]
pub struct HistoryNames {
    /// The version, in the history of the asset.
    pub version: String,
    /// The asset, in the history of the version.
    pub asset: String,
}

/// Installs a version on an asset (`Item_SoftwareVersion::add`): the
/// trash and template flags of the asset are copied, the asset and the
/// version note it in their history. `None` when it is already installed.
pub async fn install(
    pool: &MySqlPool,
    asset: &Asset,
    version_id: i32,
    entities_id: i32,
    actor: &str,
    names: &HistoryNames,
) -> Result<Option<i32>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO glpi_items_softwareversions \
         (items_id, itemtype, softwareversions_id, is_deleted_item, is_template_item, entities_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(asset.id)
    .bind(asset.kind.itemtype())
    .bind(version_id)
    .bind(asset.is_deleted)
    .bind(asset.is_template)
    .bind(entities_id)
    .execute(&mut *tx)
    .await;
    let result = match inserted {
        Ok(result) => result,
        // The unique key on the asset and the version.
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => return Ok(None),
        Err(error) => return Err(error),
    };
    let id = i32::try_from(result.last_insert_id())
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    record_both(
        &mut tx,
        asset,
        version_id,
        Action::InstallSoftware,
        actor,
        names,
    )
    .await?;
    tx.commit().await?;
    Ok(Some(id))
}

/// The version of an installation of the asset; `None` when the asset has
/// no such installation.
pub async fn installed_version(
    pool: &MySqlPool,
    kind: AssetKind,
    asset_id: i32,
    id: i32,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT softwareversions_id FROM glpi_items_softwareversions \
         WHERE id = ? AND itemtype = ? AND items_id = ?",
    )
    .bind(id)
    .bind(kind.itemtype())
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

/// Uninstalls: deletes the installation for good (`delete` forced, as the
/// massive action of the software tab), noted in the history of the asset
/// and of the version (`post_deleteFromDB`). `false` when the asset no
/// longer has this installation.
pub async fn uninstall(
    pool: &MySqlPool,
    asset: &Asset,
    id: i32,
    actor: &str,
    names: &HistoryNames,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let version_id: Option<i32> = sqlx::query_scalar(
        "SELECT softwareversions_id FROM glpi_items_softwareversions \
         WHERE id = ? AND itemtype = ? AND items_id = ? FOR UPDATE",
    )
    .bind(id)
    .bind(asset.kind.itemtype())
    .bind(asset.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(version_id) = version_id else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM glpi_items_softwareversions WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    record_both(
        &mut tx,
        asset,
        version_id,
        Action::UninstallSoftware,
        actor,
        names,
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// The asset first, then the version: the name of the other side is the
/// new value of an installation, the old one of an uninstallation.
async fn record_both(
    tx: &mut sqlx::MySqlConnection,
    asset: &Asset,
    version_id: i32,
    action: Action,
    actor: &str,
    names: &HistoryNames,
) -> Result<(), sqlx::Error> {
    let values = |name: &str| -> (String, String) {
        match action {
            Action::InstallSoftware => (String::new(), name.to_owned()),
            _ => (name.to_owned(), String::new()),
        }
    };
    let itemtype = asset.kind.itemtype();
    let (old, new) = values(&names.version);
    let on_asset = Entry::link(itemtype, asset.id, VERSION_ITEMTYPE, action, &old, &new);
    crate::history::record(tx, actor, &on_asset).await?;
    let (old, new) = values(&names.asset);
    let on_version = Entry::link(VERSION_ITEMTYPE, version_id, itemtype, action, &old, &new);
    crate::history::record(tx, actor, &on_version).await
}
