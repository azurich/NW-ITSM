//! Business rules of the assets (`RuleAsset`): the rules themselves and the
//! values their criteria compare.

use std::collections::HashMap;

use nw_domain::{
    history,
    rules::{Action, AssetRule, Criterion},
};
use sqlx::{MySqlConnection, MySqlPool};

/// A criterion or action row: rule id and three text columns.
type RuleRow = (i32, Option<String>, Option<String>, Option<String>);

/// Active rules run on the assets, in the order of the PHP code.
///
/// `CommonDBTM::assetBusinessRules` uses a rule collection on the root
/// entity: only the rules defined there run (`getRuleListCriteria`).
pub async fn asset_rules(pool: &MySqlPool) -> Result<Vec<AssetRule>, sqlx::Error> {
    let rules: Vec<(i32, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, `match`, `condition` FROM glpi_rules \
         WHERE sub_type = 'RuleAsset' AND is_active = 1 AND entities_id = 0 \
         ORDER BY ranking, id",
    )
    .fetch_all(pool)
    .await?;
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let criteria: Vec<(i32, Option<String>, i32, Option<String>)> = sqlx::query_as(
        "SELECT c.rules_id, c.criteria, c.`condition`, c.pattern FROM glpi_rulecriterias c \
         INNER JOIN glpi_rules r ON r.id = c.rules_id \
         WHERE r.sub_type = 'RuleAsset' AND r.is_active = 1 ORDER BY c.id",
    )
    .fetch_all(pool)
    .await?;
    let actions: Vec<RuleRow> = sqlx::query_as(
        "SELECT a.rules_id, a.action_type, a.field, a.value FROM glpi_ruleactions a \
         INNER JOIN glpi_rules r ON r.id = a.rules_id \
         WHERE r.sub_type = 'RuleAsset' AND r.is_active = 1 ORDER BY a.id",
    )
    .fetch_all(pool)
    .await?;

    let mut criteria_of: HashMap<i32, Vec<Criterion>> = HashMap::new();
    for (rule_id, criteria, condition, pattern) in criteria {
        criteria_of.entry(rule_id).or_default().push(Criterion {
            criteria: criteria.unwrap_or_default(),
            condition,
            pattern: pattern.unwrap_or_default(),
        });
    }
    let mut actions_of: HashMap<i32, Vec<Action>> = HashMap::new();
    for (rule_id, action_type, field, value) in actions {
        actions_of.entry(rule_id).or_default().push(Action {
            action_type: action_type.unwrap_or_default(),
            field: field.unwrap_or_default(),
            value: value.unwrap_or_default(),
        });
    }
    Ok(rules
        .into_iter()
        .map(|(id, matching, condition)| AssetRule {
            id,
            // `Rule::checkCriterias`: anything else than "AND" is "OR".
            match_all: matching.as_deref() == Some("AND"),
            condition,
            criteria: criteria_of.remove(&id).unwrap_or_default(),
            actions: actions_of.remove(&id).unwrap_or_default(),
        })
        .collect())
}

/// A tree dropdown a criterion reads: its table and parent column.
#[derive(Debug, Clone, Copy)]
pub struct Tree {
    pub table: &'static str,
    pub parent: &'static str,
}

pub const LOCATIONS: Tree = Tree {
    table: "glpi_locations",
    parent: "locations_id",
};
pub const STATES: Tree = Tree {
    table: "glpi_states",
    parent: "states_id",
};
pub const ENTITIES: Tree = Tree {
    table: "glpi_entities",
    parent: "entities_id",
};

/// Full name of a tree value as `Dropdown::getDropdownName` shows it, or
/// `""` when it does not exist.
pub async fn tree_label(
    conn: &mut MySqlConnection,
    tree: Tree,
    id: i32,
) -> Result<String, sqlx::Error> {
    let name: Option<Option<String>> = sqlx::query_scalar(&format!(
        "SELECT completename FROM {} WHERE id = ?",
        tree.table
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(name
        .flatten()
        .map(|name| history::tree_name(&name))
        .unwrap_or_default())
}

/// The value and its parents, nearest first, ending with `0` (the top of
/// the tree): the values `getSonsOf` would find it under.
pub async fn tree_parents(
    conn: &mut MySqlConnection,
    tree: Tree,
    id: i32,
) -> Result<Vec<i32>, sqlx::Error> {
    let mut parents = vec![id];
    let mut current = id;
    while current > 0 {
        let parent: Option<i32> = sqlx::query_scalar(&format!(
            "SELECT {} FROM {} WHERE id = ?",
            tree.parent, tree.table
        ))
        .bind(current)
        .fetch_optional(&mut *conn)
        .await?;
        match parent {
            Some(parent) if parent >= 0 && !parents.contains(&parent) => {
                parents.push(parent);
                current = parent;
            }
            _ => break,
        }
    }
    Ok(parents)
}

/// Login, real name and first name of a user, to format its name.
pub async fn user_names(
    conn: &mut MySqlConnection,
    id: i32,
) -> Result<Option<(Option<String>, Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT name, realname, firstname FROM glpi_users WHERE id = ?")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// `User::getIdByName`: the user with this login, if exactly one.
pub async fn user_by_login(
    conn: &mut MySqlConnection,
    login: &str,
) -> Result<Option<i32>, sqlx::Error> {
    let ids: Vec<i32> = sqlx::query_scalar("SELECT id FROM glpi_users WHERE name = ?")
        .bind(login)
        .fetch_all(conn)
        .await?;
    Ok(match ids.as_slice() {
        [id] => Some(*id),
        _ => None,
    })
}
