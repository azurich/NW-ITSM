//! Financial information of the assets (`glpi_infocoms`).

/// Entity settings filling an infocom date when an item gets a status
/// (`autofill_*` set to `STATUS_<states_id>`, `Infocom::ON_STATUS_CHANGE`).
pub const AUTOFILL_SETTINGS: [&str; 6] = [
    "autofill_buy_date",
    "autofill_delivery_date",
    "autofill_order_date",
    "autofill_use_date",
    "autofill_warranty_date",
    "autofill_decommission_date",
];

/// Whether a setting value fills a date when the item gets this status
/// (legacy `Infocom::manageDateOnStatusChange`).
pub fn fills_on_status(setting: &str, states_id: i32) -> bool {
    setting
        .strip_prefix("STATUS_")
        .is_some_and(|status| status == states_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::fills_on_status;

    #[test]
    fn status_settings() {
        assert!(fills_on_status("STATUS_2", 2));
        assert!(!fills_on_status("STATUS_2", 3));
        assert!(!fills_on_status("0", 0));
        assert!(!fills_on_status("-2", 2));
    }
}
