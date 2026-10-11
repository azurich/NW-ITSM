//! What the licenses of a software (`glpi_softwarelicenses`) do besides
//! the generic writes of the assets: their place in the tree of licenses
//! (`CommonTreeDropdown`), their validity and the validity of their
//! software (`computeValidityIndicator`, `Software::updateValidityIndicator`).

use nw_domain::{
    asset::AssetKind,
    history::{self, Change, Entry},
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
