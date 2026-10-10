//! Unicity of fields on the writes (`CommonDBTM::checkUnicity`).

use std::collections::HashMap;

use nw_db::{events, unicity as db};
use nw_domain::{asset::AssetKind, history, rights::READ, unicity::is_checked};
use serde::Serialize;
use utoipa::ToSchema;

use super::{Ref, decode};
use crate::{
    error::{ApiError, ApiResult},
    routes::auth::log_event,
    session::Session,
    state::AppState,
};

/// Values posted for a unicity check.
pub enum Posted<'a> {
    /// The whole form of the item: a criterion on a column the form does
    /// not have cannot be checked like the PHP code.
    Form(&'a HashMap<String, String>),
    /// Only some fields (an item following a computer): a criterion on
    /// other columns is skipped, as in PHP.
    Fields(&'a HashMap<String, String>),
}

/// Items with the same values, reported but not refused.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DuplicateNotice {
    /// Columns of the unicity criterion.
    pub fields: Vec<String>,
    pub items: Vec<Ref>,
}

/// Duplicates found for one criterion.
struct Found {
    fields: Vec<String>,
    values: Vec<String>,
    doubles: Vec<db::Double>,
    refuse: bool,
    notify: bool,
}

/// What the check found.
pub struct Checked {
    /// The write is refused.
    refused: Option<ApiError>,
    pub notices: Vec<DuplicateNotice>,
    events: Vec<String>,
}

impl Checked {
    /// Logs the events of the PHP code, then fails if a criterion refuses
    /// the write.
    pub async fn log_and_enforce(
        self,
        state: &AppState,
        kind: AssetKind,
        item: Option<i32>,
    ) -> ApiResult<Vec<DuplicateNotice>> {
        self.log(state, kind, item).await;
        match self.refused {
            Some(error) => Err(error),
            None => Ok(self.notices),
        }
    }

    /// Logs the events; tells whether the write is refused.
    pub async fn log(&self, state: &AppState, kind: AssetKind, item: Option<i32>) -> bool {
        for message in &self.events {
            log_event(
                state,
                &events::Event {
                    items_id: item.unwrap_or(0),
                    // The class of the item, not the event type of its forms.
                    kind: kind.itemtype(),
                    service: "inventory",
                    level: 4,
                    message,
                },
            )
            .await;
        }
        self.refused.is_some()
    }
}

/// Checks the values of an item of the entity (`item`: an existing one).
pub async fn check(
    state: &AppState,
    session: &Session,
    kind: AssetKind,
    entity: i32,
    posted: Posted<'_>,
    item: Option<i32>,
) -> ApiResult<Checked> {
    let ancestors: Vec<i32> = session.tree.ancestors_of(entity).into_iter().collect();
    let configs = db::configs(&state.pool, kind.itemtype(), entity, &ancestors).await?;
    let mut found = Vec::new();
    'configs: for config in configs {
        let (values, complete) = match &posted {
            Posted::Form(values) => (*values, true),
            Posted::Fields(values) => (*values, false),
        };
        let mut criteria = Vec::new();
        for field in config.fields.iter().filter(|field| !field.is_empty()) {
            let Some(value) = values.get(field) else {
                if complete {
                    return Err(ApiError::LegacyRequired("field_unicity"));
                }
                continue 'configs;
            };
            if !is_checked(field, value)
                || db::is_blacklisted(
                    &state.pool,
                    kind.itemtype(),
                    entity,
                    &ancestors,
                    field,
                    value,
                )
                .await?
            {
                continue 'configs;
            }
            criteria.push((field.clone(), value.clone()));
        }
        if criteria.is_empty() {
            continue;
        }
        let entities: Vec<i32> = if config.is_recursive {
            session
                .tree
                .sons_of(config.entities_id)
                .into_iter()
                .collect()
        } else {
            vec![config.entities_id]
        };
        let doubles = db::doubles(&state.pool, kind.table(), &criteria, &entities, item).await?;
        if !doubles.is_empty() {
            found.push(Found {
                fields: criteria.iter().map(|(field, _)| field.clone()).collect(),
                values: criteria.into_iter().map(|(_, value)| value).collect(),
                doubles,
                refuse: config.action_refuse,
                notify: config.action_notify,
            });
        }
    }

    // Notifications are not sent here.
    if found.iter().any(|found| found.notify)
        && db::sends_notification(&state.pool, entity, &ancestors).await?
    {
        return Err(ApiError::LegacyRequired("notifications"));
    }

    let login = session.user.name.clone().unwrap_or_default();
    let mut checked = Checked {
        refused: None,
        notices: Vec::new(),
        events: Vec::new(),
    };
    for found in found {
        let text = message(kind, session, &found);
        checked.events.push(format!(
            "{login} trying to add an item that already exists: {text}"
        ));
        if found.refuse {
            let summary = found
                .fields
                .iter()
                .zip(&found.values)
                .map(|(field, value)| format!("{field} = {}", nw_domain::text::from_stored(value)))
                .collect::<Vec<_>>()
                .join(", ");
            checked.refused.get_or_insert(ApiError::Duplicate {
                fields: found.fields.clone(),
                message: format!("another item has the same values: {summary}"),
            });
        } else {
            checked.notices.push(DuplicateNotice {
                fields: found.fields,
                items: found
                    .doubles
                    .into_iter()
                    .map(|double| Ref {
                        id: double.id,
                        name: decode(double.name),
                    })
                    .collect(),
            });
        }
    }
    Ok(checked)
}

/// `getUnicityErrorMessage`, with the English texts.
fn message(kind: AssetKind, session: &Session, found: &Found) -> String {
    let fields: Vec<String> = found
        .fields
        .iter()
        .zip(&found.values)
        .map(|(field, value)| format!("{} = {value}", label(field)))
        .collect();
    let mut text = if found.refuse {
        format!("Impossible record for {}", fields.join("&nbsp;&amp;&nbsp;"))
    } else {
        format!(
            "Item successfully added but duplicate record on {}",
            fields.join("&nbsp;&amp;&nbsp;")
        )
    };
    text.push_str("<br>Other item exist");
    for double in &found.doubles {
        let visible = session.rights.has(kind.right(), READ)
            && session
                .scope
                .can_see(double.entities_id, double.is_recursive);
        let name = double.name.clone().unwrap_or_default();
        let mut parts = Vec::new();
        if visible {
            parts.push(format!(
                "<a  href='/front/{}.form.php?id={}'  title=\"{name}\">{name}</a>",
                kind.itemtype().to_lowercase(),
                double.id
            ));
        }
        parts.push(format!("ID: {}", double.id));
        parts.push(format!(
            "Serial number: {}",
            double.serial.clone().unwrap_or_default()
        ));
        let entity = double
            .entity
            .as_deref()
            .map(history::tree_name)
            .unwrap_or_else(|| "&nbsp;".to_owned());
        parts.push(format!("Entity: {entity}"));
        if double.is_deleted {
            parts.push("Item in the trashbin".to_owned());
        }
        text.push_str(&format!("<br>[{}]", parts.join(" - ")));
    }
    text
}

/// Name of the search option the PHP code finds for a field: for a foreign
/// key, the first option showing the `name` of its table (none for the
/// trees, shown by their full name).
fn label(field: &str) -> &str {
    match field {
        "name" => "Name",
        "serial" => "Serial number",
        "otherserial" => "Inventory number",
        "contact" => "Alternate username",
        "contact_num" => "Alternate username number",
        "uuid" => "UUID",
        "brand" => "Brand",
        "number_line" => "Number of lines",
        "users_id" | "users_id_tech" => "User",
        "manufacturers_id" => "Manufacturer",
        "networks_id" => "Network",
        "autoupdatesystems_id" => "Update Source",
        "locations_id" | "states_id" | "groups_id" | "groups_id_tech" => "",
        other if other.ends_with("types_id") => "Type",
        other if other.ends_with("models_id") => "Model",
        other => other,
    }
}
