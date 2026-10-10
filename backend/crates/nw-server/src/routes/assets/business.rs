//! Business rules of the assets, run on the writes like
//! `CommonDBTM::assetBusinessRules` (see `docs/adr/0004-ecritures-du-parc.md`).

use nw_db::{MySqlPool, rules as db};
use nw_domain::{
    asset::{AssetKind, Field, FieldKind, Value},
    rules::{self, AssetRule, RuleInput, Step},
};

use crate::{
    error::{ApiError, ApiResult},
    session::Session,
};

/// Tree criteria: the key read, and its dropdown.
const TREES: [(&str, db::Tree); 3] = [
    ("locations_id", db::LOCATIONS),
    ("states_id", db::STATES),
    ("entities_id", db::ENTITIES),
];

/// The input of a legacy form or of a copy from a computer: the fields
/// posted, as text, before `_itemtype` and `_auto` are added.
#[derive(Debug, Clone, Default)]
pub struct FormInput {
    values: Vec<(String, String)>,
}

impl FormInput {
    /// Every field of the asset form, with the entity (and the id of an
    /// existing item), like the legacy forms post them.
    pub fn form(values: &[(Field, Value)], entity_id: i32, id: Option<i32>) -> Self {
        let mut input = Self::fields(values);
        input
            .values
            .push(("entities_id".to_owned(), entity_id.to_string()));
        if let Some(id) = id {
            input.values.push(("id".to_owned(), id.to_string()));
        }
        input
    }

    /// Only the given fields (the copy of a computer change to an item).
    pub fn fields(values: &[(Field, Value)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|(field, value)| (field.column.to_owned(), as_posted(value)))
                .collect(),
        }
    }
}

/// A value as the form posts it.
fn as_posted(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone().unwrap_or_default(),
        Value::Id(id) => id.to_string(),
        Value::Bool(flag) => i32::from(*flag).to_string(),
    }
}

/// Runs the rules on an input; returns the values they set on an item of
/// this type, in the order they were set. Every rule runs on creations and
/// updates alike, as in the PHP code (see `nw_domain::rules`).
pub async fn run(
    pool: &MySqlPool,
    session: &Session,
    rules: &[AssetRule],
    kind: AssetKind,
    form: &FormInput,
) -> ApiResult<Vec<(Field, Value)>> {
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let input = rule_input(pool, session, kind, form).await?;
    let steps = rules::process(rules, &input).map_err(|unsupported| {
        tracing::info!(rule = unsupported.rule_id, reason = %unsupported.reason, "business rule left to the classic interface");
        ApiError::LegacyRequired("business_rules")
    })?;

    // `executeActions` output: later values override earlier ones.
    let mut conn = pool.acquire().await?;
    let mut output: Vec<(String, String)> = Vec::new();
    let mut set = |field: &str, value: String| {
        output.retain(|(existing, _)| existing != field);
        output.push((field.to_owned(), value));
    };
    for step in steps {
        match step {
            Step::Set { field, value } => set(&field, value),
            Step::UserByLogin(logins) => {
                for login in logins {
                    if let Some(id) = db::user_by_login(&mut conn, &login).await? {
                        set("users_id", id.to_string());
                    }
                }
            }
        }
    }

    // Copied to the input: fields starting with `_` and fields the item
    // does not have are not written.
    let mut values = Vec::new();
    for (column, value) in output {
        let Some(field) = kind.field(&column).filter(|_| !column.starts_with('_')) else {
            continue;
        };
        let value = match field.kind {
            FieldKind::Text => Value::Text(Some(value)),
            FieldKind::Dropdown { .. } | FieldKind::User => Value::Id(
                value
                    .parse()
                    .map_err(|_| ApiError::LegacyRequired("business_rules"))?,
            ),
            FieldKind::Bool => Value::Bool(value != "0" && !value.is_empty()),
        };
        values.push((field, value));
    }
    Ok(values)
}

/// The values the criteria read, with the names and parents some
/// conditions compare.
async fn rule_input(
    pool: &MySqlPool,
    session: &Session,
    kind: AssetKind,
    form: &FormInput,
) -> ApiResult<RuleInput> {
    let mut input = RuleInput::default();
    input.values.extend(form.values.iter().cloned());
    input
        .values
        .insert("_itemtype".to_owned(), kind.itemtype().to_owned());
    // Manual write: not from an inventory tool.
    input.values.insert("_auto".to_owned(), "0".to_owned());

    let mut conn = pool.acquire().await?;
    // `getDropdownName` reads a missing value as the id 0 (the root entity).
    let id_of = |key: &str| {
        input
            .values
            .get(key)
            .and_then(|raw| raw.parse::<i32>().ok())
    };
    for (key, tree) in TREES {
        let id = id_of(key);
        let label = db::tree_label(&mut conn, tree, id.unwrap_or(0)).await?;
        input.labels.insert(key.to_owned(), label);
        if let Some(id) = id {
            input
                .parents
                .insert(key.to_owned(), db::tree_parents(&mut conn, tree, id).await?);
        }
    }
    if let Some(user_id) = id_of("users_id").filter(|&id| id != 0) {
        if let Some((login, realname, firstname)) = db::user_names(&mut conn, user_id).await? {
            let name = session.user_name(
                user_id,
                login.as_deref(),
                realname.as_deref(),
                firstname.as_deref(),
            );
            input.labels.insert("users_id".to_owned(), name);
        }
    }
    Ok(input)
}

/// Values for a write: those requested, then those of the rules.
pub fn merge(values: &mut Vec<(Field, Value)>, rule_values: Vec<(Field, Value)>) {
    for (field, value) in rule_values {
        values.retain(|(existing, _)| existing.column != field.column);
        values.push((field, value));
    }
}
