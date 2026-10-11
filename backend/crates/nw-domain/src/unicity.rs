//! Unicity of fields (`FieldUnicity`, `CommonDBTM::checkUnicity`): an item
//! whose values repeat those of another item of the same type is refused,
//! or only reported, depending on the configuration.

use crate::php;

/// An active unicity criterion of an item type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnicityConfig {
    pub id: i32,
    /// Entity of the criterion: the duplicates are searched there (and in
    /// its sub-entities when recursive).
    pub entities_id: i32,
    pub is_recursive: bool,
    /// Columns that must all repeat.
    pub fields: Vec<String>,
    /// The write is refused (else only reported).
    pub action_refuse: bool,
    /// A notification is sent.
    pub action_notify: bool,
}

/// The criteria used for an item of an entity: `getUnicityFieldsConfig`
/// keeps those of the first entity when sorted by decreasing id. `configs`
/// are the active criteria of the type visible from the entity.
pub fn applicable(mut configs: Vec<UnicityConfig>) -> Vec<UnicityConfig> {
    configs.sort_by_key(|config| std::cmp::Reverse(config.entities_id));
    let Some(first) = configs.first().map(|config| config.entities_id) else {
        return configs;
    };
    configs.retain(|config| config.entities_id == first);
    configs
}

/// `isForeignKeyField`: `x_id` or `x_id_y`.
pub fn is_foreign_key(field: &str) -> bool {
    let ends = field.len() > 3 && field.ends_with("_id");
    let inner = field.match_indices("_id_").any(|(at, _)| at > 0);
    ends || inner
}

/// Whether a posted value takes part in the check: a foreign key above 0,
/// any other value not empty.
pub fn is_checked(field: &str, value: &str) -> bool {
    if is_foreign_key(field) {
        // `$value > 0`
        match php::numeric(value) {
            Some(number) => number > 0.0,
            None => value > "0",
        }
    } else {
        !value.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: i32, entities_id: i32) -> UnicityConfig {
        UnicityConfig {
            id,
            entities_id,
            is_recursive: true,
            fields: vec!["serial".to_owned()],
            action_refuse: true,
            action_notify: false,
        }
    }

    #[test]
    fn criteria_of_one_entity_only() {
        let kept: Vec<i32> =
            applicable(vec![config(1, 0), config(2, 3), config(3, 1), config(4, 3)])
                .iter()
                .map(|config| config.id)
                .collect();
        assert_eq!(kept, [2, 4]);
        assert!(applicable(Vec::new()).is_empty());
    }

    #[test]
    fn foreign_keys() {
        assert!(is_foreign_key("locations_id"));
        assert!(is_foreign_key("users_id_tech"));
        assert!(!is_foreign_key("serial"));
        assert!(!is_foreign_key("_id"));
        assert!(!is_foreign_key("uuid"));
    }

    #[test]
    fn checked_values() {
        assert!(is_checked("serial", "0"));
        assert!(!is_checked("serial", ""));
        assert!(is_checked("locations_id", "3"));
        assert!(!is_checked("locations_id", "0"));
        assert!(!is_checked("users_id_tech", "0"));
    }
}
