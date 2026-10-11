//! Versions of a software (`SoftwareVersion`, a child of `Software`): their
//! fields, and how the history of the software names them.

use crate::{
    asset::{Field, FieldKind},
    history::{self, NOT_AVAILABLE},
    i18n::sprintf,
};

/// `SoftwareVersion`, stored in the `itemtype` columns.
pub const VERSION_ITEMTYPE: &str = "SoftwareVersion";

/// Fields of the form of a version, in its order (its entity part first),
/// with the search options of its own history; its sharing has none.
pub const VERSION_FIELDS: [Field; 5] = [
    Field {
        column: "is_recursive",
        search_option: crate::asset::NOT_LOGGED,
        kind: FieldKind::Bool,
    },
    Field {
        column: "comment",
        search_option: 16,
        kind: FieldKind::Text,
    },
    Field {
        column: "name",
        search_option: 2,
        kind: FieldKind::Text,
    },
    Field {
        column: "operatingsystems_id",
        search_option: 4,
        kind: FieldKind::Dropdown {
            table: "glpi_operatingsystems",
            tree: false,
        },
    },
    Field {
        column: "states_id",
        search_option: 31,
        kind: FieldKind::Dropdown {
            table: "glpi_states",
            tree: true,
        },
    },
];

/// A field of the form of a version.
pub fn version_field(column: &str) -> Option<Field> {
    VERSION_FIELDS
        .into_iter()
        .find(|field| field.column == column)
}

/// Name of a version in the history of its software (`getNameID` with the
/// id and the additional information): `"<software> - <version> (<id>)"`,
/// only `"N/A (<id>)"` for a version without name. `dash` is `"%1$s -
/// %2$s"` and `name_id` `"%1$s (%2$s)"`, in the language of the author.
pub fn version_history_name(
    dash: &str,
    name_id: &str,
    software: Option<&str>,
    version: Option<&str>,
    id: i32,
) -> String {
    let Some(version) = version.filter(|name| !name.is_empty()) else {
        return history::name_id(name_id, None, id);
    };
    let software = software
        .filter(|name| !name.is_empty())
        .unwrap_or(NOT_AVAILABLE);
    history::name_id(name_id, Some(&sprintf(dash, &[software, version])), id)
}

/// Name of a license as stored: without its leading and ending spaces
/// (`adaptTreeFieldsFromUpdateOrAdd`, PHP `trim`).
pub fn license_name(name: &str) -> &str {
    name.trim_matches([' ', '\t', '\n', '\r', '\0', '\x0B'])
}

/// Full name and level of a license under its parent (`completename` and
/// `level` of the trees); `None` for a license without parent.
pub fn license_tree(parent: Option<(&str, i32)>, name: &str) -> (String, i32) {
    match parent {
        Some((parent_name, parent_level)) => (format!("{parent_name} > {name}"), parent_level + 1),
        None => (name.to_owned(), 1),
    }
}

/// Whether a license is valid (`computeValidityIndicator`): not when it
/// has a number (`-1` for none) below its affectations. A number of 0
/// makes it invalid from the first affectation, whatever the form says.
pub fn license_is_valid(number: i64, affectations: i64) -> bool {
    !(number >= 0 && number < affectations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licenses_form_a_tree_and_count_their_affectations() {
        assert_eq!(license_name(" LIC\t"), "LIC");
        assert_eq!(license_tree(None, "A"), ("A".to_owned(), 1));
        assert_eq!(
            license_tree(Some(("P > Q", 2)), "A"),
            ("P > Q > A".to_owned(), 3)
        );
        assert!(license_is_valid(3, 3));
        assert!(!license_is_valid(1, 2));
        assert!(!license_is_valid(0, 1));
        assert!(license_is_valid(0, 0));
        assert!(license_is_valid(-1, 10));
    }

    #[test]
    fn versions_are_named_like_php() {
        let name = |software, version| {
            version_history_name("%1$s - %2$s", "%1$s (%2$s)", software, version, 7)
        };
        assert_eq!(name(Some("Office"), Some("2.0")), "Office - 2.0 (7)");
        assert_eq!(name(Some(""), Some("2.0")), "N/A - 2.0 (7)");
        assert_eq!(name(Some("Office"), None), "N/A (7)");
        assert_eq!(name(Some("Office"), Some("")), "N/A (7)");
    }

    #[test]
    fn the_form_order_is_the_history_order() {
        let columns: Vec<_> = VERSION_FIELDS.iter().map(|field| field.column).collect();
        assert_eq!(
            columns,
            [
                "is_recursive",
                "comment",
                "name",
                "operatingsystems_id",
                "states_id"
            ]
        );
        assert!(!version_field("is_recursive").unwrap().is_logged());
        assert_eq!(version_field("states_id").unwrap().search_option, 31);
    }
}
