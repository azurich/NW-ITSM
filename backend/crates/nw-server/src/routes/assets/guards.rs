//! Refuses the writes that would need a PHP feature not ported yet, so
//! that they are made in the classic interface instead of silently
//! skipping it (see `docs/adr/0004-ecritures-du-parc.md`).

use nw_db::{MySqlPool, assets, config, guards, infocoms::InfocomSettings};
use nw_domain::asset::{AssetKind, Field, Propagation, Value};

use crate::error::{ApiError, ApiResult};

/// Settings deciding what follows a computer change.
pub async fn propagation(pool: &MySqlPool) -> ApiResult<Propagation> {
    let values = config::core_values(
        pool,
        &[
            "is_contact_autoupdate",
            "is_user_autoupdate",
            "is_group_autoupdate",
            "is_location_autoupdate",
            "state_autoupdate_mode",
        ],
    )
    .await?;
    let number = |name: &str| {
        values
            .get(name)
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0)
    };
    Ok(Propagation {
        contact: number("is_contact_autoupdate") != 0,
        user: number("is_user_autoupdate") != 0,
        group: number("is_group_autoupdate") != 0,
        location: number("is_location_autoupdate") != 0,
        state: number("state_autoupdate_mode") < 0,
    })
}

/// Checks a move to the trash or a restoration.
pub async fn check_trash(pool: &MySqlPool) -> ApiResult<()> {
    check_plugins(pool).await
}

/// Checks a creation.
pub async fn check_create(
    pool: &MySqlPool,
    kind: AssetKind,
    infocoms: &InfocomSettings,
) -> ApiResult<()> {
    check_plugins(pool).await?;
    check_unicity(
        pool,
        &[kind],
        infocoms.auto_create || infocoms.any_status_date(),
    )
    .await
}

/// Columns a write changes on an item.
pub fn changed_columns(current: &assets::Current, values: &[(Field, Value)]) -> Vec<&'static str> {
    values
        .iter()
        .filter(|(field, new)| {
            current
                .value(field.column)
                .is_some_and(|old| new.differs_from(old))
        })
        .map(|(field, _)| field.column)
        .collect()
}

/// Checks an update, with the items that follow a computer change
/// (`connected`: their types).
pub async fn check_update(
    pool: &MySqlPool,
    kind: AssetKind,
    current: &assets::Current,
    changed: &[&str],
    connected: &[AssetKind],
    infocoms: &InfocomSettings,
) -> ApiResult<()> {
    if changed.is_empty() {
        return Ok(());
    }
    check_plugins(pool).await?;
    if changed.contains(&"is_recursive") {
        if current.value("is_recursive") == Some(&Value::Bool(true)) {
            // Legacy CommonDBTM::canUnrecurs checks the items of other
            // entities that use this one.
            return Err(ApiError::LegacyRequired("unrecursive"));
        }
        // The sharing is copied to the linked items, infocoms excepted.
        if !assets::forwarded_items(pool, kind, current.id)
            .await?
            .is_empty()
        {
            return Err(ApiError::LegacyRequired("linked_items"));
        }
    }
    let mut kinds = vec![kind];
    kinds.extend(connected);
    let infocom_writes = (changed.contains(&"states_id") && infocoms.any_status_date())
        || changed.contains(&"is_recursive");
    check_unicity(pool, &kinds, infocom_writes).await
}

/// The hooks of the PHP plugins (`pre_item_add`, `item_update`...) do not
/// run here.
async fn check_plugins(pool: &MySqlPool) -> ApiResult<()> {
    if guards::has_active_plugins(pool).await? {
        Err(ApiError::LegacyRequired("plugins"))
    } else {
        Ok(())
    }
}

/// Unicity criteria of the written types, infocoms included when they may
/// be written too.
async fn check_unicity(pool: &MySqlPool, kinds: &[AssetKind], infocoms: bool) -> ApiResult<()> {
    let mut itemtypes: Vec<&str> = kinds.iter().map(|kind| kind.itemtype()).collect();
    if infocoms {
        itemtypes.push("Infocom");
    }
    for itemtype in itemtypes {
        if guards::has_field_unicity(pool, itemtype).await? {
            return Err(ApiError::LegacyRequired("field_unicity"));
        }
    }
    Ok(())
}
