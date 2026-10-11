//! Profile rights (`glpi_profilerights`).
//!
//! Each right is a bitmask stored per profile and per module name
//! (`computer`, `ticket`, ...). The bit values come from `inc/define.php`.

use std::collections::HashMap;

pub const READ: i32 = 1;
pub const UPDATE: i32 = 2;
pub const CREATE: i32 = 4;
pub const DELETE: i32 = 8;
pub const PURGE: i32 = 16;
pub const ALLSTANDARDRIGHT: i32 = 31;
pub const READNOTE: i32 = 32;
pub const UPDATENOTE: i32 = 64;
pub const UNLOCK: i32 = 128;

/// The interface a profile gives access to (`glpi_profiles.interface`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    /// Full interface for technicians and administrators.
    Central,
    /// Simplified self-service interface.
    Helpdesk,
}

impl Interface {
    /// Parses the legacy column value. Any other value means the profile
    /// cannot log in (legacy `Session::getCurrentInterface` returns false).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "central" => Some(Self::Central),
            "helpdesk" => Some(Self::Helpdesk),
            _ => None,
        }
    }
}

/// Rights kept for helpdesk profiles (legacy `Profile::$helpdesk_rights`).
/// Every other right is dropped by `Profile::cleanProfile`.
const HELPDESK_RIGHTS: &[&str] = &[
    "create_ticket_on_login",
    "changetemplates_id",
    "followup",
    "helpdesk_hardware",
    "helpdesk_item_type",
    "knowbase",
    "password_update",
    "personalization",
    "problemtemplates_id",
    "reminder_public",
    "reservation",
    "rssfeed_public",
    "show_group_hardware",
    "task",
    "ticket",
    "ticket_cost",
    "ticket_status",
    "tickettemplates_id",
    "ticketvalidation",
    "accessibility",
    "changefont",
    "changezoom",
    "useshortcuts",
];

/// Rights of the active profile.
#[derive(Debug, Clone, Default)]
pub struct Rights(HashMap<String, i32>);

impl Rights {
    /// Builds the rights of a profile from its `(name, rights)` rows.
    pub fn new(interface: Interface, rows: impl IntoIterator<Item = (String, i32)>) -> Self {
        Self(
            rows.into_iter()
                .filter(|(name, _)| {
                    interface == Interface::Central || HELPDESK_RIGHTS.contains(&name.as_str())
                })
                .collect(),
        )
    }

    /// Raw bitmask of a module, `0` when absent.
    pub fn get(&self, module: &str) -> i32 {
        self.0.get(module).copied().unwrap_or(0)
    }

    /// Every `(module, bitmask)` pair.
    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> {
        self.0
            .iter()
            .map(|(module, rights)| (module.as_str(), *rights))
    }

    /// Same as legacy `Session::haveRight`: true when at least one bit of
    /// `right` is set for the module.
    pub fn has(&self, module: &str, right: i32) -> bool {
        self.get(module) & right != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<(String, i32)> {
        vec![
            ("computer".to_owned(), READ | UPDATE),
            ("ticket".to_owned(), 1024),
        ]
    }

    #[test]
    fn central_profile_keeps_every_right() {
        let rights = Rights::new(Interface::Central, rows());
        assert!(rights.has("computer", READ));
        assert!(rights.has("computer", UPDATE));
        assert!(!rights.has("computer", CREATE));
        assert!(!rights.has("monitor", READ));
    }

    #[test]
    fn helpdesk_profile_drops_asset_rights() {
        let rights = Rights::new(Interface::Helpdesk, rows());
        assert!(!rights.has("computer", READ));
        assert_eq!(rights.get("ticket"), 1024);
    }

    #[test]
    fn any_bit_is_enough() {
        let rights = Rights::new(Interface::Central, rows());
        assert!(rights.has("computer", ALLSTANDARDRIGHT));
    }

    #[test]
    fn unknown_interface_cannot_log_in() {
        assert_eq!(Interface::parse(""), None);
        assert_eq!(Interface::parse("central"), Some(Interface::Central));
    }
}
