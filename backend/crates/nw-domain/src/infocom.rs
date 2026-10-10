//! Financial information of the assets (`glpi_infocoms`): created with the
//! items when configured, and dates filled when an item gets a status.

use crate::php;

/// An infocom date an entity setting can fill (`autofill_*`), in the order
/// of `Infocom::getAutoManagemendDatesFields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoDate {
    /// Entity setting (`glpi_entities` column).
    pub setting: &'static str,
    /// Infocom column.
    pub field: &'static str,
    /// Search option of the field on the item, for its history.
    pub search_option: i32,
    /// Stored with the time (`TIMESTAMP`), else only the day (`DATE`).
    pub with_time: bool,
}

pub const AUTO_DATES: [AutoDate; 6] = [
    AutoDate {
        setting: "autofill_buy_date",
        field: "buy_date",
        search_option: 37,
        with_time: false,
    },
    AutoDate {
        setting: "autofill_use_date",
        field: "use_date",
        search_option: 38,
        with_time: false,
    },
    AutoDate {
        setting: "autofill_delivery_date",
        field: "delivery_date",
        search_option: 142,
        with_time: false,
    },
    AutoDate {
        setting: "autofill_warranty_date",
        field: "warranty_date",
        search_option: 123,
        with_time: false,
    },
    AutoDate {
        setting: "autofill_order_date",
        field: "order_date",
        search_option: 124,
        with_time: false,
    },
    AutoDate {
        setting: "autofill_decommission_date",
        field: "decommission_date",
        search_option: 159,
        with_time: true,
    },
];

/// Entity setting of the default alert of a new infocom.
pub const DEFAULT_ALERT_SETTING: &str = "default_infocom_alert";

/// Value of `Entity::getUsedConfig` when no entity defines the setting.
pub const UNDEFINED_SETTING: &str = "-2";

/// `Alert::END`: alert at the end of the warranty.
pub const ALERT_END: i32 = 2;

/// Whether a setting value fills its date when the item gets this status
/// (`Infocom::manageDateOnStatusChange`: `/STATUS_(.*)/` and a loose
/// comparison with the status id).
pub fn fills_on_status(setting: &str, states_id: i32) -> bool {
    let Some(at) = setting.find("STATUS_") else {
        return false;
    };
    // `.` stops at a newline.
    let status = setting[at + "STATUS_".len()..]
        .split('\n')
        .next()
        .unwrap_or_default();
    php::loose_eq(status, &states_id.to_string())
}

/// Dates filled when an item of an entity gets a status; `setting` gives
/// the used value of an entity setting.
pub fn status_dates<'a>(
    setting: impl Fn(&str) -> Option<&'a str>,
    states_id: i32,
) -> Vec<AutoDate> {
    AUTO_DATES
        .into_iter()
        .filter(|date| setting(date.setting).is_some_and(|value| fills_on_status(value, states_id)))
        .collect()
}

/// `$CFG_GLPI['auto_create_infocoms']`, read as a PHP boolean.
pub fn auto_create(config: Option<&str>) -> bool {
    config.is_some_and(|value| !php::is_empty(value))
}

/// What filling a date changes on an existing infocom, compared like
/// `CommonDBTM::update` and `updateInDB` do: the field is always rewritten
/// (the stored value never equals the date with its time), but the history
/// only has an entry when the stored value really changes.
pub fn date_update(date: AutoDate, stored: Option<&str>, now: &str) -> DateUpdate {
    let new_stored = if date.with_time {
        now
    } else {
        now.get(..10).unwrap_or(now)
    };
    DateUpdate {
        changed: stored != Some(now),
        logged: stored != Some(new_stored),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateUpdate {
    /// Written by `CommonDBTM::update` (the infocom gets a new `date_mod`).
    pub changed: bool,
    /// Has a history entry on the item.
    pub logged: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_settings() {
        assert!(fills_on_status("STATUS_2", 2));
        assert!(fills_on_status("STATUS_02", 2));
        assert!(!fills_on_status("STATUS_2", 3));
        assert!(!fills_on_status("0", 0));
        assert!(!fills_on_status("-2", 2));
        assert!(!fills_on_status("1", 1));
    }

    #[test]
    fn dates_of_a_status() {
        let settings = |setting: &str| match setting {
            "autofill_use_date" | "autofill_buy_date" => Some("STATUS_1"),
            "autofill_warranty_date" => Some("STATUS_2"),
            _ => Some("0"),
        };
        let fields: Vec<_> = status_dates(settings, 1)
            .iter()
            .map(|date| date.field)
            .collect();
        assert_eq!(fields, ["buy_date", "use_date"]);
        assert!(status_dates(settings, 3).is_empty());
    }

    #[test]
    fn updates_of_an_existing_date() {
        // Measured with the PHP code: a date already set to the day is
        // rewritten without history.
        let now = "2026-10-10 14:42:34";
        let use_date = AUTO_DATES[1];
        assert_eq!(
            date_update(use_date, Some("2026-10-10"), now),
            DateUpdate {
                changed: true,
                logged: false
            }
        );
        assert_eq!(
            date_update(use_date, Some("2026-10-09"), now),
            DateUpdate {
                changed: true,
                logged: true
            }
        );
        assert_eq!(
            date_update(use_date, None, now),
            DateUpdate {
                changed: true,
                logged: true
            }
        );
        let decommission = AUTO_DATES[5];
        assert_eq!(
            date_update(decommission, Some(now), now),
            DateUpdate {
                changed: false,
                logged: false
            }
        );
    }

    #[test]
    fn auto_creation_setting() {
        assert!(auto_create(Some("1")));
        assert!(!auto_create(Some("0")));
        assert!(!auto_create(Some("")));
        assert!(!auto_create(None));
    }
}
