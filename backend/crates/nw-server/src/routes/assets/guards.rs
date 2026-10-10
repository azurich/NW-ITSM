//! Refuses the writes that would need a PHP feature not ported yet, so
//! that they are made in the classic interface instead of silently
//! skipping it (see `docs/adr/0004-ecritures-du-parc.md`).

use nw_db::{MySqlPool, assets, config, guards, infocoms::InfocomSettings, unicity};
use nw_domain::{
    asset::{AssetKind, DEVICE_FIELDS, Field, Propagation, Value},
    entity::EntityTree,
    infocom::AUTO_DATES,
};

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
pub async fn check_create(pool: &MySqlPool, infocoms: &InfocomSettings) -> ApiResult<()> {
    check_plugins(pool).await?;
    if infocoms.auto_create || infocoms.any_status_date() {
        check_side_unicity(pool, "Infocom", &INFOCOM_INPUT).await?;
    }
    Ok(())
}

/// Columns a write changes on an item.
pub fn changed_columns(current: &assets::Current, values: &[(Field, Value)]) -> Vec<&'static str> {
    values
        .iter()
        .filter(|(field, new)| {
            current
                .value(field.column)
                .is_some_and(|old| field.changes(old, new))
        })
        .map(|(field, _)| field.column)
        .collect()
}

/// Checks an update that changes these columns.
pub async fn check_update(
    pool: &MySqlPool,
    tree: &EntityTree,
    kind: AssetKind,
    current: &assets::Current,
    changed: &[&str],
    propagation: Propagation,
    infocoms: &InfocomSettings,
) -> ApiResult<()> {
    if changed.is_empty() {
        return Ok(());
    }
    check_plugins(pool).await?;
    if changed.contains(&"is_recursive") {
        if current.value("is_recursive") == Some(&Value::Bool(true)) {
            // `CommonDBTM::canUnrecurs`: the items of other entities using
            // this one would no longer see it.
            let mut entities: Vec<i32> =
                tree.ancestors_of(current.entities_id).into_iter().collect();
            entities.push(current.entities_id);
            let blockers = assets::unshare_blockers(pool, kind, current.id, &entities).await?;
            if !blockers.is_empty() {
                return Err(ApiError::SharingInUse(blockers.join(", ")));
            }
        }
        // Copying the entity to linked items of another entity is not
        // ported, only the sharing.
        if !assets::entity_mismatches(pool, kind, current.id, current.entities_id)
            .await?
            .is_empty()
        {
            return Err(ApiError::LegacyRequired("linked_items"));
        }
    }
    let infocom_writes = (changed.contains(&"states_id") && infocoms.any_status_date())
        || changed.contains(&"is_recursive");
    if infocom_writes {
        check_side_unicity(pool, "Infocom", &INFOCOM_INPUT).await?;
    }
    if kind == AssetKind::Computer && !propagation.to_devices(changed).is_empty() {
        let device_input: Vec<&str> = DEVICE_FIELDS
            .iter()
            .map(|field| field.column)
            .chain(["id"])
            .collect();
        check_side_unicity(pool, "Item_DeviceSimcard", &device_input).await?;
    }
    Ok(())
}

/// Fields an infocom write posts (creation, dates, sharing).
const INFOCOM_INPUT: [&str; 13] = [
    "itemtype",
    "items_id",
    "id",
    "entities_id",
    "is_recursive",
    "alert",
    "_transfer",
    AUTO_DATES[0].field,
    AUTO_DATES[1].field,
    AUTO_DATES[2].field,
    AUTO_DATES[3].field,
    AUTO_DATES[4].field,
    AUTO_DATES[5].field,
];

/// Unicity of the items written along (infocoms, SIM cards): not ported.
/// A criterion only on fields such a write posts could apply to it.
async fn check_side_unicity(pool: &MySqlPool, itemtype: &str, posted: &[&str]) -> ApiResult<()> {
    let applies = unicity::active_fields(pool, itemtype)
        .await?
        .iter()
        .any(|fields| {
            let fields: Vec<&str> = fields
                .iter()
                .map(String::as_str)
                .filter(|field| !field.is_empty())
                .collect();
            !fields.is_empty() && fields.iter().all(|field| posted.contains(field))
        });
    if applies {
        Err(ApiError::LegacyRequired("field_unicity"))
    } else {
        Ok(())
    }
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
