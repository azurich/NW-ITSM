//! History of changes (`glpi_logs`), in the format of `Log::history`.

/// The PHP code keeps the first 180 characters of each value.
const MAX_VALUE_CHARS: usize = 180;

/// Shown by the PHP code for an empty or missing dropdown value.
pub const NO_VALUE: &str = "&nbsp;";

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

/// `linked_action` of an entry (`Log::HISTORY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A field of the item changed.
    FieldChange,
    /// A field of a component (`Item_Device*`) of the item changed.
    UpdateDevice,
    CreateItem,
    /// Moved to the trash.
    DeleteItem,
    /// Restored from the trash.
    RestoreItem,
    /// Moved to the trash, for an item managed by an inventory tool.
    LockItem,
    /// Restored, for an item managed by an inventory tool.
    UnlockItem,
    /// A link to another item was added (`HISTORY_ADD_RELATION`).
    AddRelation,
}

impl Action {
    pub fn code(self) -> i32 {
        match self {
            Self::FieldChange => 0,
            Self::UpdateDevice => 2,
            Self::DeleteItem => 13,
            Self::RestoreItem => 14,
            Self::AddRelation => 15,
            Self::CreateItem => 20,
            Self::LockItem => 26,
            Self::UnlockItem => 27,
        }
    }
}

/// A row of `glpi_logs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub itemtype: String,
    pub items_id: i32,
    pub itemtype_link: String,
    pub action: Action,
    pub change: Change,
}

impl Entry {
    /// A field of the item changed.
    pub fn field(itemtype: &str, items_id: i32, change: Change) -> Self {
        Self {
            itemtype: itemtype.to_owned(),
            items_id,
            itemtype_link: String::new(),
            action: Action::FieldChange,
            change,
        }
    }

    /// Something happened to the whole item (creation, trash...). The PHP
    /// code stores `0` as the link type of these entries.
    pub fn item(itemtype: &str, items_id: i32, action: Action) -> Self {
        Self {
            itemtype: itemtype.to_owned(),
            items_id,
            itemtype_link: "0".to_owned(),
            action,
            change: Change::new(0, "", ""),
        }
    }

    /// A link to another item was added, recorded on one side: the type
    /// and the name of the other side (`CommonDBRelation::post_addItem`).
    pub fn relation(itemtype: &str, items_id: i32, other_itemtype: &str, other_name: &str) -> Self {
        Self {
            itemtype: itemtype.to_owned(),
            items_id,
            itemtype_link: other_itemtype.to_owned(),
            action: Action::AddRelation,
            change: Change::new(0, "", other_name),
        }
    }

    /// A field of a component changed, recorded on the item that holds it:
    /// the link is `<component type>#<field>` and the values are raw ids.
    pub fn device_update(
        itemtype: &str,
        items_id: i32,
        device_itemtype: &str,
        field: &str,
        old: i32,
        new: i32,
    ) -> Self {
        Self {
            itemtype: itemtype.to_owned(),
            items_id,
            itemtype_link: format!("{device_itemtype}#{field}"),
            action: Action::UpdateDevice,
            change: Change::new(0, &old.to_string(), &new.to_string()),
        }
    }
}

/// Author of a change: `"<name> (<id>)"`, the name formatted with the
/// author's own display preferences.
pub fn actor(user_id: i32, formatted_name: &str) -> String {
    format!("{formatted_name} ({user_id})")
}

/// A dropdown value as written by `Log::constructHistory`:
/// `"<name> (<id>)"`, with `&nbsp;` when there is no value.
pub fn dropdown_value(name: Option<&str>, id: i32) -> String {
    let name = name.filter(|name| !name.is_empty()).unwrap_or(NO_VALUE);
    format!("{name} ({id})")
}

/// Name of an item in the history of its links (`getNameID` with the id
/// forced): `name_id` is `"%1$s (%2$s)"` in the language of the author,
/// `N/A` stands for an empty name.
pub fn name_id(name_id: &str, name: Option<&str>, id: i32) -> String {
    let name = name
        .filter(|name| !name.is_empty())
        .unwrap_or(NOT_AVAILABLE);
    crate::i18n::sprintf(name_id, &[name, &id.to_string()])
}

/// `NOT_AVAILABLE` of the PHP code.
pub const NOT_AVAILABLE: &str = "N/A";

/// Full name of a tree dropdown as shown by the PHP code: the separator,
/// stored raw, is HTML-encoded (`getTreeValueCompleteName`).
pub fn tree_name(completename: &str) -> String {
    completename.replace(" > ", " &gt; ")
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

    #[test]
    fn entries_match_the_php_rows() {
        // Recorded from Computer::update and Monitor::add/delete in PHP.
        let created = Entry::item("Monitor", 2, Action::CreateItem);
        assert_eq!(
            (created.itemtype_link.as_str(), created.action.code()),
            ("0", 20)
        );
        assert_eq!(created.change, Change::new(0, "", ""));

        let field = Entry::field("Computer", 1, Change::new(7, "", "jdupont"));
        assert_eq!((field.itemtype_link.as_str(), field.action.code()), ("", 0));

        let device =
            Entry::device_update("Computer", 1, "Item_DeviceProcessor", "locations_id", 1, 2);
        assert_eq!(device.itemtype_link, "Item_DeviceProcessor#locations_id");
        assert_eq!(device.action.code(), 2);
        assert_eq!(device.change, Change::new(0, "1", "2"));
    }

    #[test]
    fn dropdown_values() {
        assert_eq!(dropdown_value(Some("Dell"), 1), "Dell (1)");
        assert_eq!(dropdown_value(None, 0), "&nbsp; (0)");
        assert_eq!(dropdown_value(Some(""), 3), "&nbsp; (3)");
        assert_eq!(
            tree_name("Siège > Salle serveur"),
            "Siège &gt; Salle serveur"
        );
    }
}
