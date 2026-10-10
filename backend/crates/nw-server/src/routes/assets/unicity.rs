//! Unicity of fields on the writes (`CommonDBTM::checkUnicity`).

use std::collections::HashMap;

use nw_db::{events, unicity as db};
use nw_domain::{
    asset::AssetKind,
    history,
    i18n::{self, Source, text},
    php,
    rights::READ,
    unicity::is_checked,
};
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
                // A field the form leaves out (an empty size on creation)
                // skips the criterion, as in PHP; another one cannot be
                // checked like the PHP code.
                let left_out = kind
                    .field(field)
                    .is_some_and(|field| field.form_default().is_none());
                if complete && !left_out {
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
    let language = session.preferences.language.as_str();
    let mut checked = Checked {
        refused: None,
        notices: Vec::new(),
        events: Vec::new(),
    };
    for found in found {
        let message = message(kind, session, &found);
        checked.events.push(i18n::sprintf(
            &i18n::tr(language, text::DUPLICATE_EVENT),
            &[&login, &message],
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

/// `getUnicityErrorMessage`, in the language of the session.
fn message(kind: AssetKind, session: &Session, found: &Found) -> String {
    let language = session.preferences.language.as_str();
    let tr = |source: Source| i18n::tr(language, source);
    let fields: Vec<String> = found
        .fields
        .iter()
        .zip(&found.values)
        .map(|(field, value)| {
            let label = label(field, &tr);
            i18n::sprintf(&tr(text::EQUALS), &[&label, value])
        })
        .collect();
    let format = if found.refuse {
        text::IMPOSSIBLE_RECORD
    } else {
        text::DUPLICATE_RECORD
    };
    let mut message = i18n::sprintf(&tr(format), &[&fields.join("&nbsp;&amp;&nbsp;")]);
    message.push_str("<br>");
    message.push_str(&tr(text::OTHER_ITEM_EXIST));
    let dash = tr(text::DASH);
    let colon = tr(text::COLON);
    for double in &found.doubles {
        let visible = session.rights.has(kind.right(), READ)
            && session
                .scope
                .can_see(double.entities_id, double.is_recursive);
        let mut parts = Vec::new();
        if visible {
            parts.push(link(kind, session, double));
        }
        let serial = double.serial.clone().unwrap_or_default();
        let entity = double
            .entity
            .as_deref()
            .map(history::tree_name)
            .unwrap_or_else(|| "&nbsp;".to_owned());
        parts.push(i18n::sprintf(
            &colon,
            &[&tr(text::ID), &double.id.to_string()],
        ));
        // A value equal to `NOT_AVAILABLE` is skipped.
        if serial != NOT_AVAILABLE {
            parts.push(i18n::sprintf(&colon, &[&tr(text::SERIAL), &serial]));
        }
        parts.push(i18n::sprintf(&colon, &[&tr(text::ENTITY), &entity]));
        if double.is_deleted {
            parts.push(tr(text::IN_TRASHBIN));
        }
        let double_text = parts
            .into_iter()
            .reduce(|text, part| i18n::sprintf(&dash, &[&text, &part]))
            .unwrap_or_default();
        message.push_str(&format!("<br>[{double_text}]"));
    }
    message
}

/// `NOT_AVAILABLE` of the PHP code, the name of an item without name.
const NOT_AVAILABLE: &str = "N/A";

/// `CommonDBTM::getLink` of an item the user can see.
fn link(kind: AssetKind, session: &Session, double: &db::Double) -> String {
    let name = double
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| NOT_AVAILABLE.to_owned());
    // `getNameID`.
    let label = if session.preferences.names.show_ids {
        let format = i18n::tr(&session.preferences.language, text::NAME_ID);
        i18n::sprintf(&format, &[&name, &double.id.to_string()])
    } else {
        name.clone()
    };
    let title = if name == NOT_AVAILABLE {
        String::new()
    } else {
        format!(" title=\"{}\"", php::htmlentities(&name))
    };
    format!(
        "<a  href='/front/{}.form.php?id={}' {title}>{label}</a>",
        kind.itemtype().to_lowercase(),
        double.id
    )
}

/// Name of the search option the PHP code finds for a field: for a foreign
/// key, the first option showing the `name` of its table (none for the
/// trees and the entity, shown by their full name), else the first option
/// on a column of that name, whatever its table.
fn label(field: &str, tr: &impl Fn(Source) -> String) -> String {
    let source = match field {
        "name" => text::NAME,
        "serial" => text::SERIAL,
        "otherserial" => text::INVENTORY_NUMBER,
        "contact" => text::CONTACT,
        "contact_num" => text::CONTACT_NUMBER,
        "uuid" => text::UUID,
        "brand" => text::BRAND,
        "number_line" => text::NUMBER_LINE,
        "comment" => text::LOCATION_COMMENTS,
        "is_recursive" => text::CHILD_ENTITIES,
        "users_id" | "users_id_tech" => text::USER,
        "manufacturers_id" => text::MANUFACTURER,
        "networks_id" => text::NETWORK,
        "autoupdatesystems_id" => text::UPDATE_SOURCE,
        "size" => text::SIZE,
        "have_micro" => text::MICROPHONE,
        "have_speaker" => text::SPEAKERS,
        "have_subd" => text::SUB_D,
        "have_bnc" => text::BNC,
        "have_dvi" => text::DVI,
        "have_pivot" => text::PIVOT,
        "have_hdmi" => text::HDMI,
        "have_displayport" => text::DISPLAYPORT,
        "is_global" => text::GLOBAL_MANAGEMENT,
        "have_serial" => text::SERIAL_PORT,
        "have_parallel" => text::PARALLEL,
        "have_usb" => text::USB,
        "have_ethernet" => text::ETHERNET,
        "have_wifi" => text::WIFI,
        "memory_size" => text::MEMORY,
        "init_pages_counter" => text::INITIAL_PAGE_COUNTER,
        "last_pages_counter" => text::PAGE_COUNTER,
        "phonepowersupplies_id" => text::POWER_SUPPLY,
        "have_headset" => text::HEADSET,
        "have_hp" => text::SPEAKER,
        // `Memory (Mio)`.
        "ram" => return i18n::sprintf(&tr(text::NAME_ID), &[&tr(text::MEMORY), &tr(text::MIO)]),
        other if other.ends_with("types_id") => text::TYPE,
        other if other.ends_with("models_id") => text::MODEL,
        _ => return String::new(),
    };
    tr(source)
}
