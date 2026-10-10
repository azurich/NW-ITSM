//! Business rules of the assets (`RuleAsset`), run by the PHP code on every
//! manual add or update (`CommonDBTM::assetBusinessRules`).
//!
//! Same engine as `RuleCollection::processAllRules` and `Rule::process`:
//! every active rule is tried in order, on the input of the legacy form;
//! the actions of the matching rules fill the output, later rules
//! overriding earlier ones. What cannot be reproduced identically (plugin
//! criteria and actions, some regular expressions) is reported as
//! [`Unsupported`], so that the write is left to the classic interface.
//!
//! Legacy bug kept on purpose: `assetBusinessRules` passes the add/update
//! condition as the `$params` of `processAllRules` instead of its
//! `$options`, so the condition is never applied and every active rule
//! runs on both creations and updates. Both interfaces must give the same
//! result: it is reproduced here (measured against the PHP code).

pub mod pcre;

use std::collections::HashMap;

use crate::{php, text::from_stored};

/// `Rule::PATTERN_*` and `Rule::REGEX_*` conditions.
pub mod condition {
    pub const IS: i32 = 0;
    pub const IS_NOT: i32 = 1;
    pub const CONTAIN: i32 = 2;
    pub const NOT_CONTAIN: i32 = 3;
    pub const BEGIN: i32 = 4;
    pub const END: i32 = 5;
    pub const REGEX_MATCH: i32 = 6;
    pub const REGEX_NOT_MATCH: i32 = 7;
    pub const EXISTS: i32 = 8;
    pub const DOES_NOT_EXIST: i32 = 9;
    pub const FIND: i32 = 10;
    pub const UNDER: i32 = 11;
    pub const NOT_UNDER: i32 = 12;
    pub const IS_EMPTY: i32 = 30;
}

/// `Rule::RULE_WILDCARD`: a pattern matching anything.
const WILDCARD: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub criteria: String,
    pub condition: i32,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub action_type: String,
    pub field: String,
    pub value: String,
}

/// An active rule, with its criteria and actions in their stored order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRule {
    pub id: i32,
    /// `AND` matching: every criterion must match (else `OR`).
    pub match_all: bool,
    /// Operations the rule is meant for (`RuleAsset::ONADD` 1, `ONUPDATE`
    /// 2): ignored by the PHP code, see the module documentation.
    pub condition: i32,
    pub criteria: Vec<Criterion>,
    pub actions: Vec<Action>,
}

/// How a criterion value is read (`RuleAsset::getCriterias`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CriterionKind {
    /// Compared as entered.
    Text,
    /// Yes/no: shown translated by the conditions comparing text.
    YesNo,
    /// Id of a tree dropdown: its full name for the conditions comparing
    /// text; `UNDER` works on its parents.
    Tree,
    /// Id of a plain dropdown: its name for the conditions comparing text.
    Dropdown,
    /// Id of a user: the formatted name for the conditions comparing text.
    User,
}

fn criterion_kind(criteria: &str) -> Option<CriterionKind> {
    Some(match criteria {
        "_auto" => CriterionKind::YesNo,
        "_itemtype" | "comment" | "contact" | "contact_num" => CriterionKind::Text,
        "states_id" | "locations_id" | "entities_id" => CriterionKind::Tree,
        "manufacturer" => CriterionKind::Dropdown,
        "users_id" => CriterionKind::User,
        _ => return None,
    })
}

/// Fields the rules may set (`RuleAsset::getActions`).
pub const ACTION_FIELDS: [&str; 8] = [
    "states_id",
    "locations_id",
    "users_id",
    "_affect_user_by_regex",
    "groups_id",
    "users_id_tech",
    "groups_id_tech",
    "comment",
];

/// Values the rules see: the input of the legacy form, ids written as
/// decimal numbers, text as stored. A missing key reads as `""`.
#[derive(Debug, Clone, Default)]
pub struct RuleInput {
    pub values: HashMap<String, String>,
    /// Name of the dropdown and user values (`Dropdown::getDropdownName`,
    /// `getUserName`), used by the conditions comparing text. A tree
    /// dropdown has its full name with the separator encoded (`&gt;`).
    pub labels: HashMap<String, String>,
    /// Tree values with their parents, nearest first: `UNDER` conditions.
    pub parents: HashMap<String, Vec<i32>>,
}

impl RuleInput {
    fn raw(&self, key: &str) -> &str {
        self.values.get(key).map_or("", String::as_str)
    }

    fn label(&self, key: &str) -> &str {
        self.labels.get(key).map_or("", String::as_str)
    }
}

/// What a matching rule does to the output, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Sets a field.
    Set { field: String, value: String },
    /// Sets `users_id` to the user with each login found, if any (the last
    /// one found wins).
    UserByLogin(Vec<String>),
}

/// A rule that cannot be run identically here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub rule_id: i32,
    pub reason: String,
}

/// Runs the rules in their order; returns the steps of the matching rules.
pub fn process(rules: &[AssetRule], input: &RuleInput) -> Result<Vec<Step>, Unsupported> {
    let mut steps = Vec::new();
    for rule in rules {
        // `Rule::validateCriterias`: a rule without criteria never runs.
        if rule.criteria.is_empty() {
            continue;
        }
        let unsupported = |reason: String| Unsupported {
            rule_id: rule.id,
            reason,
        };
        let mut regex_results: Vec<Vec<String>> = Vec::new();
        let matched = if rule.match_all {
            let mut all = true;
            for criterion in &rule.criteria {
                if !check(criterion, input, &mut regex_results).map_err(unsupported)? {
                    all = false;
                    break;
                }
            }
            all
        } else {
            let mut any = false;
            for criterion in &rule.criteria {
                if check(criterion, input, &mut regex_results).map_err(unsupported)? {
                    any = true;
                    break;
                }
            }
            any
        };
        if matched {
            for action in &rule.actions {
                steps.push(execute(action, &regex_results).map_err(unsupported)?);
            }
        }
    }
    Ok(steps)
}

/// `Rule::checkCriteria`.
fn check(
    criterion: &Criterion,
    input: &RuleInput,
    regex_results: &mut Vec<Vec<String>>,
) -> Result<bool, String> {
    use condition::*;

    // `RuleCriteria::match`: checked before anything else.
    if criterion.pattern == WILDCARD || criterion.condition == FIND {
        return Ok(true);
    }
    let key = criterion.criteria.as_str();
    let kind = criterion_kind(key).ok_or_else(|| format!("criterion {key}"))?;
    // `Rule::getCriteriaValue`: names instead of ids for the conditions
    // comparing text.
    let compares_text = !matches!(
        criterion.condition,
        IS | IS_NOT | EXISTS | DOES_NOT_EXIST | UNDER | NOT_UNDER
    );
    let value = match (compares_text, kind) {
        (false, _) | (true, CriterionKind::Text) => input.raw(key),
        (true, CriterionKind::Tree | CriterionKind::Dropdown | CriterionKind::User) => {
            input.label(key)
        }
        (true, CriterionKind::YesNo) => return Err(format!("text condition on {key}")),
    };

    let pattern = php::trim(&criterion.pattern);
    Ok(match criterion.condition {
        EXISTS => !php::is_empty(value),
        DOES_NOT_EXIST => php::is_empty(value),
        IS => php::loose_eq(&php::lower(value), &php::lower(pattern)),
        IS_NOT => !php::loose_eq(&php::lower(value), &php::lower(pattern)),
        UNDER | NOT_UNDER => {
            if kind != CriterionKind::Tree {
                return Err(format!("`under` on {key}"));
            }
            let ancestor: i32 = pattern
                .parse()
                .map_err(|_| format!("`under` pattern {pattern}"))?;
            let under = input
                .parents
                .get(key)
                .is_some_and(|parents| parents.contains(&ancestor));
            under == (criterion.condition == UNDER)
        }
        END => match pcre::Pattern::compile(&format!("/{pattern}$/i")) {
            Ok(regex) => regex.is_match(value).map_err(regex_error)?,
            Err(pcre::Error::Invalid) => false,
            Err(error) => return Err(regex_error(error)),
        },
        BEGIN => !php::is_empty(pattern) && php::contains_ci(value, pattern, true),
        CONTAIN => !php::is_empty(pattern) && php::contains_ci(value, pattern, false),
        NOT_CONTAIN => !php::is_empty(pattern) && !php::contains_ci(value, pattern, false),
        REGEX_MATCH => match pcre::Pattern::compile(&format!("{}i", from_stored(pattern))) {
            Ok(regex) => match regex.captures_all(value).map_err(regex_error)? {
                Some(found) => {
                    add_regex_result(regex_results, found);
                    true
                }
                None => false,
            },
            Err(pcre::Error::Invalid) => false,
            Err(error) => return Err(regex_error(error)),
        },
        // `preg_match(...) == 0` is also true when PHP rejects the pattern.
        REGEX_NOT_MATCH => match pcre::Pattern::compile(&format!("{}i", from_stored(pattern))) {
            Ok(regex) => !regex.is_match(value).map_err(regex_error)?,
            Err(pcre::Error::Invalid) => true,
            Err(error) => return Err(regex_error(error)),
        },
        // Global criteria, checked later by the PHP code (always true here).
        IS_EMPTY => true,
        _ => false,
    })
}

fn regex_error(error: pcre::Error) -> String {
    match error {
        pcre::Error::Invalid => "invalid regular expression".to_owned(),
        pcre::Error::Unsupported(reason) => reason,
    }
}

/// Results of a criterion are combined with those of the previous ones:
/// every earlier result is extended with each new one.
fn add_regex_result(results: &mut Vec<Vec<String>>, found: Vec<String>) {
    if results.is_empty() {
        results.push(found);
    } else {
        for result in results.iter_mut() {
            result.extend(found.iter().cloned());
        }
    }
}

/// `RuleAsset::executeActions`.
fn execute(action: &Action, regex_results: &[Vec<String>]) -> Result<Step, String> {
    let field = action.field.as_str();
    if !ACTION_FIELDS.contains(&field) {
        return Err(format!("action on {field}"));
    }
    match action.action_type.as_str() {
        "assign" => Ok(Step::Set {
            field: field.to_owned(),
            value: action.value.clone(),
        }),
        "regex_result" if field == "_affect_user_by_regex" => Ok(Step::UserByLogin(
            regex_results
                .iter()
                .map(|result| regex_result_by_id(&action.value, result))
                // `$res != null`: an empty login is skipped.
                .filter(|login| !login.is_empty())
                .collect(),
        )),
        "regex_result" => Ok(Step::Set {
            field: field.to_owned(),
            value: match regex_results.first() {
                Some(result) => regex_result_by_id(&action.value, result),
                None => action.value.clone(),
            },
        }),
        other => Err(format!("action type {other}")),
    }
}

/// `RuleAction::getRegexResultById`: `#0` to `#9` replaced by the values
/// found, in the order they appear.
fn regex_result_by_id(value: &str, result: &[String]) -> String {
    if result.is_empty() {
        return value.to_owned();
    }
    let digits: Vec<char> = value
        .char_indices()
        .filter(|&(at, c)| {
            c == '#' && value[at + 1..].starts_with(|next: char| next.is_ascii_digit())
        })
        .filter_map(|(at, _)| value[at + 1..].chars().next())
        .collect();
    let mut replaced = value.to_owned();
    for digit in digits {
        let index = digit.to_digit(10).expect("ASCII digit") as usize;
        let found = result.get(index).map_or("", String::as_str);
        replaced = replaced.replace(&format!("#{digit}"), found);
    }
    replaced
}

#[cfg(test)]
mod tests {
    use super::{condition::*, *};

    fn criterion(criteria: &str, condition: i32, pattern: &str) -> Criterion {
        Criterion {
            criteria: criteria.to_owned(),
            condition,
            pattern: pattern.to_owned(),
        }
    }

    fn assign(field: &str, value: &str) -> Action {
        Action {
            action_type: "assign".to_owned(),
            field: field.to_owned(),
            value: value.to_owned(),
        }
    }

    fn rule(id: i32, criteria: Vec<Criterion>, actions: Vec<Action>) -> AssetRule {
        AssetRule {
            id,
            match_all: true,
            condition: 3,
            criteria,
            actions,
        }
    }

    fn input(values: &[(&str, &str)]) -> RuleInput {
        RuleInput {
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..RuleInput::default()
        }
    }

    fn set(field: &str, value: &str) -> Step {
        Step::Set {
            field: field.to_owned(),
            value: value.to_owned(),
        }
    }

    /// The three rules of a fresh install: inventory tools only.
    fn default_rules() -> Vec<AssetRule> {
        ["/(.*)@/", "/(.*),/", "/(.*)/"]
            .iter()
            .enumerate()
            .map(|(i, pattern)| {
                rule(
                    9 + i as i32,
                    vec![
                        criterion("_itemtype", IS, "Computer"),
                        criterion("_auto", IS, "1"),
                        criterion("contact", REGEX_MATCH, pattern),
                    ],
                    vec![Action {
                        action_type: "regex_result".to_owned(),
                        field: "_affect_user_by_regex".to_owned(),
                        value: "#0".to_owned(),
                    }],
                )
            })
            .collect()
    }

    #[test]
    fn default_rules_ignore_manual_writes() {
        let manual = input(&[
            ("_itemtype", "Computer"),
            ("_auto", "0"),
            ("contact", "jdoe@x"),
        ]);
        assert_eq!(process(&default_rules(), &manual).unwrap(), []);
    }

    #[test]
    fn default_rules_on_inventory_input() {
        let inventory = input(&[
            ("_itemtype", "Computer"),
            ("_auto", "1"),
            ("contact", "jdoe@x"),
        ]);
        assert_eq!(
            process(&default_rules(), &inventory).unwrap(),
            [
                Step::UserByLogin(vec!["jdoe".to_owned()]),
                // "/(.*),/" does not match. "/(.*)/" also matches the empty
                // end of the text: its result set is ["jdoe@x", ""].
                Step::UserByLogin(vec!["jdoe@x".to_owned()]),
            ]
        );
    }

    #[test]
    fn conditions_of_operations_are_ignored_like_php() {
        // A rule meant for creations also runs on updates (legacy bug).
        let add_only = AssetRule {
            condition: 1,
            ..rule(
                1,
                vec![criterion("contact", IS, "x")],
                vec![assign("locations_id", "3")],
            )
        };
        let x = input(&[("contact", "X")]);
        assert_eq!(
            process(&[add_only], &x).unwrap(),
            [set("locations_id", "3")]
        );
    }

    #[test]
    fn rules_without_criteria_and_matching() {
        let x = input(&[("contact", "X")]);
        assert_eq!(
            process(&[rule(1, vec![], vec![assign("comment", "c")])], &x).unwrap(),
            []
        );

        let either = AssetRule {
            match_all: false,
            ..rule(
                2,
                vec![
                    criterion("contact", IS, "nobody"),
                    criterion("comment", EXISTS, ""),
                ],
                vec![assign("comment", "seen")],
            )
        };
        assert_eq!(process(std::slice::from_ref(&either), &x).unwrap(), []);
        let commented = input(&[("contact", "X"), ("comment", "hello")]);
        assert_eq!(
            process(&[either], &commented).unwrap(),
            [set("comment", "seen")]
        );
    }

    #[test]
    fn conditions() {
        let values = RuleInput {
            values: [
                ("contact", "Jean Dupont"),
                ("states_id", "2"),
                ("locations_id", "5"),
                ("comment", "0"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            labels: [
                ("states_id", "En stock"),
                ("locations_id", "Siège &gt; Salle 1"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            parents: [("locations_id".to_owned(), vec![5, 1, 0])].into(),
        };
        let matches = |criteria: &str, condition: i32, pattern: &str| {
            let rule = rule(
                1,
                vec![criterion(criteria, condition, pattern)],
                vec![assign("comment", "hit")],
            );
            !process(&[rule], &values).unwrap().is_empty()
        };
        assert!(matches("contact", IS, "jean dupont"));
        assert!(matches("states_id", IS, "02"));
        assert!(matches("contact", IS_NOT, "x"));
        assert!(matches("contact", CONTAIN, "DUP"));
        assert!(!matches("contact", CONTAIN, "0"));
        assert!(matches("contact", BEGIN, "jean"));
        assert!(matches("contact", NOT_CONTAIN, "paul"));
        assert!(matches("contact", END, "pont"));
        assert!(matches("contact", END, "p.nt"));
        assert!(matches("states_id", CONTAIN, "stock"));
        // The separator of the full name is encoded, the pattern is not.
        assert!(matches("locations_id", REGEX_MATCH, "/siège .* salle/"));
        assert!(!matches("locations_id", REGEX_MATCH, "/siège &gt; salle/"));
        assert!(matches("locations_id", UNDER, "1"));
        assert!(!matches("locations_id", NOT_UNDER, "1"));
        assert!(matches("locations_id", NOT_UNDER, "7"));
        assert!(!matches("comment", EXISTS, ""));
        assert!(matches("comment", DOES_NOT_EXIST, ""));
        assert!(matches("contact", REGEX_NOT_MATCH, "/^paul/"));
        // PHP rejects these patterns: no match, and "does not match".
        assert!(!matches("contact", REGEX_MATCH, "jean"));
        assert!(matches("contact", REGEX_NOT_MATCH, "jean"));
        assert!(!matches("contact", END, "a/b"));
        assert!(matches("plugin_criterion", IS, "*"));
        assert!(matches("contact", 99, "*"));
        assert!(!matches("contact", 99, "x"));
    }

    #[test]
    fn regex_results_feed_the_actions() {
        let rule = rule(
            1,
            vec![
                criterion("contact", REGEX_MATCH, "/^(\\w+)\\.(\\w+)$/"),
                criterion("comment", REGEX_MATCH, "/room (\\d+)/"),
            ],
            vec![
                Action {
                    action_type: "regex_result".to_owned(),
                    field: "comment".to_owned(),
                    value: "#1 #0 in #2 #9".to_owned(),
                },
                Action {
                    action_type: "regex_result".to_owned(),
                    field: "_affect_user_by_regex".to_owned(),
                    value: "#0".to_owned(),
                },
            ],
        );
        let found = input(&[("contact", "jean.dupont"), ("comment", "room 12")]);
        assert_eq!(
            process(&[rule], &found).unwrap(),
            [
                set("comment", "dupont jean in 12 "),
                Step::UserByLogin(vec!["jean".to_owned()])
            ]
        );
    }

    #[test]
    fn later_rules_see_the_original_input() {
        let rules = [
            rule(
                1,
                vec![criterion("contact", IS, "a")],
                vec![assign("comment", "first")],
            ),
            rule(
                2,
                vec![criterion("comment", IS, "first")],
                vec![assign("comment", "second")],
            ),
        ];
        assert_eq!(
            process(&rules, &input(&[("contact", "a")])).unwrap(),
            [set("comment", "first")]
        );
    }

    #[test]
    fn plugin_rules_are_unsupported() {
        let unknown_criterion = rule(4, vec![criterion("_plugin_tag", IS, "x")], vec![]);
        let unknown_action = rule(
            5,
            vec![criterion("contact", IS, "")],
            vec![assign("name", "x")],
        );
        let unknown_type = rule(
            6,
            vec![criterion("contact", IS, "")],
            vec![Action {
                action_type: "append".to_owned(),
                field: "comment".to_owned(),
                value: "x".to_owned(),
            }],
        );
        let translated = rule(7, vec![criterion("_auto", CONTAIN, "Yes")], vec![]);
        for (rule, id) in [
            (unknown_criterion, 4),
            (unknown_action, 5),
            (unknown_type, 6),
            (translated, 7),
        ] {
            let error = process(&[rule], &RuleInput::default()).unwrap_err();
            assert_eq!(error.rule_id, id, "{}", error.reason);
        }
    }

    #[test]
    fn regex_results_replace_digits_in_order() {
        let result = ["a".to_owned(), "b".to_owned()];
        assert_eq!(regex_result_by_id("#0-#1-#0-#5", &result), "a-b-a-");
        assert_eq!(regex_result_by_id("#0", &[]), "#0");
        assert_eq!(regex_result_by_id("no digit #x", &result), "no digit #x");
    }
}
