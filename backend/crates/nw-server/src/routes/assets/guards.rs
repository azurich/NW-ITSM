//! Refuses the writes that would need a PHP feature not ported yet, so
//! that they are made in the classic interface instead of silently
//! skipping it (see `docs/adr/0004-ecritures-du-parc.md`).

use nw_db::{MySqlPool, assets, config, guards};
use nw_domain::{
    asset::{AssetKind, Field, Propagation, Value},
    entity::{EntityTree, used_config},
    infocom::{AUTOFILL_SETTINGS, fills_on_status},
    rules::Operation,
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
pub async fn check_create(
    pool: &MySqlPool,
    tree: &EntityTree,
    kind: AssetKind,
    entity_id: i32,
    values: &[(Field, Value)],
) -> ApiResult<()> {
    check_plugins(pool).await?;
    check_rules(pool, &[kind], Operation::Add).await?;
    check_unicity(pool, kind).await?;
    let auto_create = config::core_values(pool, &["auto_create_infocoms"]).await?;
    if auto_create.get("auto_create_infocoms").map(String::as_str) == Some("1") {
        return Err(ApiError::LegacyRequired("infocom_creation"));
    }
    if let Some(states_id) = value_of(values, "states_id") {
        check_status_dates(pool, tree, entity_id, states_id).await?;
    }
    Ok(())
}

/// Checks an update, with the items that would follow a computer change.
pub async fn check_update(
    pool: &MySqlPool,
    tree: &EntityTree,
    kind: AssetKind,
    current: &assets::Current,
    values: &[(Field, Value)],
    propagation: Propagation,
) -> ApiResult<()> {
    let changed: Vec<&str> = values
        .iter()
        .filter(|(field, new)| {
            current
                .value(field.column)
                .is_some_and(|old| new.differs_from(old))
        })
        .map(|(field, _)| field.column)
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    check_plugins(pool).await?;
    if current.value("is_recursive") == Some(&Value::Bool(true))
        && changed.contains(&"is_recursive")
    {
        // Legacy CommonDBTM::canUnrecurs checks the items of other entities
        // that use this one.
        return Err(ApiError::LegacyRequired("unrecursive"));
    }

    let mut kinds = vec![kind];
    if kind == AssetKind::Computer {
        kinds.extend(assets::propagation_targets(pool, current.id, propagation, &changed).await?);
    }
    check_rules(pool, &kinds, Operation::Update).await?;
    check_unicity(pool, kind).await?;

    if changed.contains(&"states_id") {
        if let Some(states_id) = value_of(values, "states_id") {
            check_status_dates(pool, tree, current.entities_id, states_id).await?;
            if kinds.len() > 1 && propagation.state {
                // The connected items may be in other entities.
                check_status_dates_anywhere(pool, states_id).await?;
            }
        }
    }
    Ok(())
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

async fn check_rules(pool: &MySqlPool, kinds: &[AssetKind], operation: Operation) -> ApiResult<()> {
    let rules = guards::active_asset_rules(pool).await?;
    let applies = kinds.iter().any(|kind| {
        rules
            .iter()
            .any(|rule| rule.may_apply_to_manual_write(kind.itemtype(), operation))
    });
    if applies {
        Err(ApiError::LegacyRequired("business_rules"))
    } else {
        Ok(())
    }
}

async fn check_unicity(pool: &MySqlPool, kind: AssetKind) -> ApiResult<()> {
    if guards::has_field_unicity(pool, kind.itemtype()).await? {
        Err(ApiError::LegacyRequired("field_unicity"))
    } else {
        Ok(())
    }
}

/// Infocom dates filled when the item of this entity gets the status.
async fn check_status_dates(
    pool: &MySqlPool,
    tree: &EntityTree,
    entity_id: i32,
    states_id: i32,
) -> ApiResult<()> {
    let settings = guards::entity_settings(pool, &AUTOFILL_SETTINGS).await?;
    for index in 0..AUTOFILL_SETTINGS.len() {
        let used = used_config(tree, entity_id, |id| {
            settings
                .get(&id)
                .and_then(|values| values[index].as_deref())
        });
        if used.is_some_and(|setting| fills_on_status(setting, states_id)) {
            return Err(ApiError::LegacyRequired("infocom_dates"));
        }
    }
    Ok(())
}

/// Infocom dates filled by this status in any entity.
async fn check_status_dates_anywhere(pool: &MySqlPool, states_id: i32) -> ApiResult<()> {
    let settings = guards::entity_settings(pool, &AUTOFILL_SETTINGS).await?;
    let filled = settings
        .values()
        .flatten()
        .flatten()
        .any(|setting| fills_on_status(setting, states_id));
    if filled {
        Err(ApiError::LegacyRequired("infocom_dates"))
    } else {
        Ok(())
    }
}

fn value_of(values: &[(Field, Value)], column: &str) -> Option<i32> {
    values
        .iter()
        .find(|(field, _)| field.column == column)
        .map(|(_, value)| value.id())
}
