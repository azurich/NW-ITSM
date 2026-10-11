//! What the licenses of a software (`glpi_softwarelicenses`) do besides
//! the generic writes of the assets: their place in the tree of licenses
//! (`CommonTreeDropdown`), their validity and the validity of their
//! software (`computeValidityIndicator`, `Software::updateValidityIndicator`).

use nw_domain::{
    asset::AssetKind,
    history::{self, Action, Change, Entry},
    software::{license_is_valid, license_tree},
};
use sqlx::{MySqlConnection, MySqlPool};

/// Search option of the full name and of the parent of a license.
const PARENT_OPTION: i32 = 13;
/// Search option of the validity of a license.
const VALID_OPTION: i32 = 9;
/// Search option of the validity of a software (its licenses).
const SOFTWARE_VALID_OPTION: i32 = 63;

/// Sets the full name and the level of a license from its parent and its
/// name (`adaptTreeFieldsFromUpdateOrAdd`). Returns the old and the new
/// full name.
pub(crate) async fn place_in_tree(
    conn: &mut MySqlConnection,
    id: i32,
) -> Result<(Option<String>, String), sqlx::Error> {
    let (parent_id, name, completename): (i32, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT softwarelicenses_id, name, completename FROM glpi_softwarelicenses WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let parent: Option<(Option<String>, i32)> = if parent_id > 0 {
        sqlx::query_as("SELECT completename, level FROM glpi_softwarelicenses WHERE id = ?")
            .bind(parent_id)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        None
    };
    let name = name.unwrap_or_default();
    let (new, level) = license_tree(
        parent
            .as_ref()
            .map(|(completename, level)| (completename.as_deref().unwrap_or_default(), *level)),
        &name,
    );
    sqlx::query(
        "UPDATE glpi_softwarelicenses SET softwarelicenses_id = ?, completename = ?, level = ? \
         WHERE id = ?",
    )
    // A parent that does not exist is none.
    .bind(if parent.is_some() { parent_id } else { 0 })
    .bind(&new)
    .bind(level)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok((completename, new))
}

/// Places a license changed in the tree again, after a change of its name
/// or of its parent, and logs its new full name (raw, its separators
/// encoded) after the fields of the form.
pub(crate) async fn replace_in_tree(
    conn: &mut MySqlConnection,
    id: i32,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let (old, new) = place_in_tree(conn, id).await?;
    if old.as_deref() != Some(new.as_str()) {
        let change = Change::new(
            PARENT_OPTION,
            &history::tree_name(old.as_deref().unwrap_or_default()),
            &history::tree_name(&new),
        );
        let entry = Entry::field(AssetKind::SoftwareLicense.itemtype(), id, change);
        crate::history::record(conn, actor, &entry).await?;
    }
    Ok(())
}

/// Affectations of a license counted by its validity
/// (`Item_SoftwareLicense::countForLicense` in every entity): outside the
/// trash, of items that exist and are neither in the trash nor templates.
pub async fn affectations(conn: &mut MySqlConnection, id: i32) -> Result<i64, sqlx::Error> {
    let itemtypes: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT itemtype FROM glpi_items_softwarelicenses WHERE softwarelicenses_id = ?",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let mut count = 0;
    for itemtype in itemtypes {
        let Some(kind) = AssetKind::from_itemtype(&itemtype) else {
            continue;
        };
        let mut sql = format!(
            "SELECT COUNT(*) FROM glpi_items_softwarelicenses l JOIN {table} i ON i.id = l.items_id \
             WHERE l.itemtype = ? AND l.softwarelicenses_id = ? AND l.is_deleted = 0 \
             AND i.is_deleted = 0",
            table = kind.table()
        );
        if kind.has_templates() {
            sql.push_str(" AND i.is_template = 0");
        }
        let found: i64 = sqlx::query_scalar(&sql)
            .bind(&itemtype)
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        count += found;
    }
    Ok(count)
}

/// Computes the validity of a license from its number and its
/// affectations, as each update does (`prepareInputForUpdate`): a change
/// is logged. Returns its software when it changed, whose validity follows
/// ([`update_software_validity`], after the other effects of the update).
pub(crate) async fn update_validity(
    conn: &mut MySqlConnection,
    id: i32,
    actor: &str,
) -> Result<Option<i32>, sqlx::Error> {
    let (number, is_valid, software): (i32, bool, i32) = sqlx::query_as(
        "SELECT number, is_valid, softwares_id FROM glpi_softwarelicenses WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let valid = license_is_valid(i64::from(number), affectations(conn, id).await?);
    if valid == is_valid {
        return Ok(None);
    }
    sqlx::query("UPDATE glpi_softwarelicenses SET is_valid = ?, date_mod = NOW() WHERE id = ?")
        .bind(valid)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let change = Change::new(
        VALID_OPTION,
        &i32::from(is_valid).to_string(),
        &i32::from(valid).to_string(),
    );
    let entry = Entry::field(AssetKind::SoftwareLicense.itemtype(), id, change);
    crate::history::record(conn, actor, &entry).await?;
    Ok(Some(software))
}

/// The validity of a software: none of its licenses is invalid
/// (`Software::updateValidityIndicator`, the trash included). A change is
/// written like a form, with its history and a new `date_mod`.
pub(crate) async fn update_software_validity(
    conn: &mut MySqlConnection,
    software: i32,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let stored: Option<bool> =
        sqlx::query_scalar("SELECT is_valid FROM glpi_softwares WHERE id = ?")
            .bind(software)
            .fetch_optional(&mut *conn)
            .await?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let invalid: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM glpi_softwarelicenses WHERE softwares_id = ? AND is_valid = 0",
    )
    .bind(software)
    .fetch_one(&mut *conn)
    .await?;
    let valid = invalid == 0;
    if valid == stored {
        return Ok(());
    }
    sqlx::query("UPDATE glpi_softwares SET is_valid = ?, date_mod = NOW() WHERE id = ?")
        .bind(valid)
        .bind(software)
        .execute(&mut *conn)
        .await?;
    let change = Change::new(
        SOFTWARE_VALID_OPTION,
        &i32::from(stored).to_string(),
        &i32::from(valid).to_string(),
    );
    let entry = Entry::field(AssetKind::Software.itemtype(), software, change);
    crate::history::record(conn, actor, &entry).await
}

/// The license and the licenses under it (`getSonsOf`): none of them can
/// become its parent.
pub async fn with_descendants(pool: &MySqlPool, id: i32) -> Result<Vec<i32>, sqlx::Error> {
    let mut found = vec![id];
    let mut next = vec![id];
    while !next.is_empty() {
        let mut children = Vec::new();
        for parent in next {
            let rows: Vec<i32> = sqlx::query_scalar(
                "SELECT id FROM glpi_softwarelicenses WHERE softwarelicenses_id = ?",
            )
            .bind(parent)
            .fetch_all(pool)
            .await?;
            children.extend(rows.into_iter().filter(|child| !found.contains(child)));
        }
        found.extend(&children);
        next = children;
    }
    Ok(found)
}

/// Whether licenses are under this one (moved by its trash or purge).
pub async fn has_children(pool: &MySqlPool, id: i32) -> Result<bool, sqlx::Error> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM glpi_softwarelicenses WHERE softwarelicenses_id = ? LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

/// The software of a license.
pub async fn software_of(conn: &mut MySqlConnection, id: i32) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT softwares_id FROM glpi_softwarelicenses WHERE id = ?")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// An item a license is affected to.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AffectedItem {
    /// Id of the affectation.
    pub id: i32,
    pub itemtype: String,
    pub items_id: i32,
    pub name: Option<String>,
    pub entities_id: i32,
    pub entity: Option<String>,
    pub serial: Option<String>,
    pub otherserial: Option<String>,
    pub is_deleted: bool,
}

/// The items of the types with software a license is affected to, visible
/// to the session (`Item_SoftwareLicense::showForLicense`), by entity and
/// name.
pub async fn affected_items(
    pool: &MySqlPool,
    id: i32,
    scope: &nw_domain::entity::EntityScope,
) -> Result<Vec<AffectedItem>, sqlx::Error> {
    let mut sql = sqlx::QueryBuilder::<sqlx::MySql>::new("SELECT * FROM (");
    let kinds: Vec<AssetKind> = AssetKind::ALL
        .into_iter()
        .filter(|kind| kind.has_software())
        .collect();
    for (index, kind) in kinds.iter().enumerate() {
        if index > 0 {
            sql.push(" UNION ALL ");
        }
        sql.push(format_args!(
            "SELECT l.id, l.itemtype, l.items_id, i.name, i.entities_id, e.completename AS entity, \
             i.serial, i.otherserial, CAST(i.is_deleted AS SIGNED) <> 0 AS is_deleted \
             FROM glpi_items_softwarelicenses l JOIN {table} i ON i.id = l.items_id \
             LEFT JOIN glpi_entities e ON e.id = i.entities_id \
             WHERE l.itemtype = ",
            table = kind.table()
        ));
        sql.push_bind(kind.itemtype())
            .push(" AND l.softwarelicenses_id = ")
            .push_bind(id)
            .push(" AND l.is_deleted = 0 AND i.is_template = 0");
        crate::scope::restrict_entities(&mut sql, "i", scope);
    }
    sql.push(") affected ORDER BY entity, name, id");
    sql.build_query_as().fetch_all(pool).await
}

/// A license affected to an asset.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AffectedLicense {
    /// Id of the affectation.
    pub id: i32,
    pub license_id: i32,
    pub name: Option<String>,
    pub softwares_id: i32,
    pub software: Option<String>,
    pub number: i32,
    pub is_valid: bool,
    pub expire: Option<chrono::NaiveDate>,
}

/// The licenses of an asset visible to the session, by software and name.
pub async fn asset_licenses(
    pool: &MySqlPool,
    kind: AssetKind,
    asset_id: i32,
    scope: &nw_domain::entity::EntityScope,
) -> Result<Vec<AffectedLicense>, sqlx::Error> {
    let mut sql = sqlx::QueryBuilder::<sqlx::MySql>::new(
        "SELECT l.id, sl.id AS license_id, sl.name, sl.softwares_id, s.name AS software, sl.number, \
         sl.is_valid, sl.expire \
         FROM glpi_items_softwarelicenses l \
         JOIN glpi_softwarelicenses sl ON sl.id = l.softwarelicenses_id \
         LEFT JOIN glpi_softwares s ON s.id = sl.softwares_id \
         WHERE l.itemtype = ",
    );
    sql.push_bind(kind.itemtype())
        .push(" AND l.items_id = ")
        .push_bind(asset_id)
        .push(" AND l.is_deleted = 0");
    crate::scope::restrict_entities(&mut sql, "sl", scope);
    sql.push(" ORDER BY s.name, sl.name, l.id");
    sql.build_query_as().fetch_all(pool).await
}

/// An affectation: the license and the item.
#[derive(Debug, Clone, Copy)]
pub struct Affectation {
    pub license: i32,
    pub kind: AssetKind,
    pub item: i32,
}

/// The affectation of an id, when its item has software.
pub async fn affectation(pool: &MySqlPool, id: i32) -> Result<Option<Affectation>, sqlx::Error> {
    let row: Option<(i32, String, i32)> = sqlx::query_as(
        "SELECT softwarelicenses_id, itemtype, items_id FROM glpi_items_softwarelicenses WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(license, itemtype, item)| {
        let kind = AssetKind::from_itemtype(&itemtype).filter(|kind| kind.has_software())?;
        Some(Affectation {
            license,
            kind,
            item,
        })
    }))
}

/// The validity of a license once an item is added to or removed from its
/// affectations (`None`: unchanged), to know beforehand whether its
/// update runs.
pub async fn validity_after(
    pool: &MySqlPool,
    license: i32,
    kind: AssetKind,
    item: i32,
    adding: bool,
) -> Result<Option<bool>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let (number, is_valid): (i32, bool) =
        sqlx::query_as("SELECT number, is_valid FROM glpi_softwarelicenses WHERE id = ?")
            .bind(license)
            .fetch_one(&mut *conn)
            .await?;
    let count = affectations(&mut conn, license).await?;
    // The item counts when it exists, outside the trash and the templates.
    let mut counted = format!(
        "SELECT COUNT(*) FROM {} WHERE id = ? AND is_deleted = 0",
        kind.table()
    );
    if kind.has_templates() {
        counted.push_str(" AND is_template = 0");
    }
    let counts: i64 = sqlx::query_scalar(&counted)
        .bind(item)
        .fetch_one(&mut *conn)
        .await?;
    let count = if adding {
        count + counts
    } else {
        count - counts
    };
    let valid = license_is_valid(i64::from(number), count);
    Ok((valid != is_valid).then_some(valid))
}

/// Affects a license to an item (`Item_SoftwareLicense::add`): the
/// validity of the license and of its software first (`post_addItem`),
/// then the history of both sides (`HISTORY_ADD_RELATION`).
pub async fn affect(
    pool: &MySqlPool,
    license: i32,
    kind: AssetKind,
    item: i32,
    actor: &str,
    names: &RelationNames,
) -> Result<i32, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO glpi_items_softwarelicenses (items_id, itemtype, softwarelicenses_id) \
         VALUES (?, ?, ?)",
    )
    .bind(item)
    .bind(kind.itemtype())
    .bind(license)
    .execute(&mut *tx)
    .await?;
    let id = i32::try_from(result.last_insert_id())
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    if let Some(software) = update_validity(&mut tx, license, actor).await? {
        update_software_validity(&mut tx, software, actor).await?;
    }
    record_relation(
        &mut tx,
        license,
        kind,
        item,
        Action::AddRelation,
        actor,
        names,
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Removes an affectation for good (the purge of the license and asset
/// tabs): the validity first (`post_deleteFromDB`), then the history of
/// both sides (`HISTORY_DEL_RELATION`). `false` when it no longer exists.
pub async fn unaffect(
    pool: &MySqlPool,
    id: i32,
    affectation: Affectation,
    actor: &str,
    names: &RelationNames,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query(
        "DELETE FROM glpi_items_softwarelicenses WHERE id = ? AND softwarelicenses_id = ? \
         AND itemtype = ? AND items_id = ?",
    )
    .bind(id)
    .bind(affectation.license)
    .bind(affectation.kind.itemtype())
    .bind(affectation.item)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Ok(false);
    }
    if let Some(software) = update_validity(&mut tx, affectation.license, actor).await? {
        update_software_validity(&mut tx, software, actor).await?;
    }
    record_relation(
        &mut tx,
        affectation.license,
        affectation.kind,
        affectation.item,
        Action::DeleteRelation,
        actor,
        names,
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Each side as named in the history of the other.
#[derive(Debug, Clone)]
pub struct RelationNames {
    /// The license, in the history of the item (with its software).
    pub license: String,
    /// The item, in the history of the license.
    pub item: String,
}

/// The item first, then the license: the name of the other side is the
/// new value of an addition, the old one of a removal.
async fn record_relation(
    conn: &mut MySqlConnection,
    license: i32,
    kind: AssetKind,
    item: i32,
    action: Action,
    actor: &str,
    names: &RelationNames,
) -> Result<(), sqlx::Error> {
    let values = |name: &str| -> (String, String) {
        match action {
            Action::AddRelation => (String::new(), name.to_owned()),
            _ => (name.to_owned(), String::new()),
        }
    };
    let license_itemtype = AssetKind::SoftwareLicense.itemtype();
    let (old, new) = values(&names.license);
    let on_item = Entry::link(kind.itemtype(), item, license_itemtype, action, &old, &new);
    crate::history::record(conn, actor, &on_item).await?;
    let (old, new) = values(&names.item);
    let on_license = Entry::link(
        license_itemtype,
        license,
        kind.itemtype(),
        action,
        &old,
        &new,
    );
    crate::history::record(conn, actor, &on_license).await
}

/// Name of a license and of its software, as the history of an item
/// names the license (`getNameID` with the software).
pub async fn names_of(
    pool: &MySqlPool,
    id: i32,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT sl.name, s.name FROM glpi_softwarelicenses sl \
         LEFT JOIN glpi_softwares s ON s.id = sl.softwares_id WHERE sl.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
