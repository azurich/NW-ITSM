//! History of changes (`glpi_logs`), in the format of `Log::history`.

/// The PHP code keeps the first 180 characters of each value.
const MAX_VALUE_CHARS: usize = 180;

/// A changed field, identified like in the PHP code by the search option
/// that displays it (`rawSearchOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub search_option: i32,
    pub old: String,
    pub new: String,
}

impl Change {
    pub fn new(search_option: i32, old: &str, new: &str) -> Self {
        Self {
            search_option,
            old: truncate(old),
            new: truncate(new),
        }
    }
}

/// Author of a change: `"<name> (<id>)"`, the name formatted with the
/// author's own display preferences.
pub fn actor(user_id: i32, formatted_name: &str) -> String {
    format!("{formatted_name} ({user_id})")
}

fn truncate(value: &str) -> String {
    value.chars().take(MAX_VALUE_CHARS).collect()
}

/// Search options of the user fields that the API changes.
pub mod user {
    pub const ITEMTYPE: &str = "User";
    pub const LANGUAGE: i32 = 17;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_cut_at_180_characters() {
        let long = "é".repeat(200);
        let change = Change::new(17, &long, "en_GB");
        assert_eq!(change.old.chars().count(), 180);
        assert_eq!(change.new, "en_GB");
    }

    #[test]
    fn actor_includes_the_id() {
        assert_eq!(actor(2, "itsm"), "itsm (2)");
    }
}
