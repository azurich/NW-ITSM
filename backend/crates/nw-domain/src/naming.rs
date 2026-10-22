//! Display names, as formatted by the legacy `DbUtils::formatUserName`.

/// Order of the first and last names (`names_format` preference).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NamesFormat {
    /// "Lastname Firstname" (`User::REALNAME_BEFORE`, default).
    #[default]
    RealnameFirst,
    /// "Firstname Lastname" (`User::FIRSTNAME_BEFORE`).
    FirstnameFirst,
}

impl NamesFormat {
    pub fn from_legacy(value: i32) -> Self {
        if value == 1 {
            Self::FirstnameFirst
        } else {
            Self::RealnameFirst
        }
    }
}

/// Display preferences of the viewer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NameDisplay {
    pub format: NamesFormat,
    /// `is_ids_visible` preference: append the id to names.
    pub show_ids: bool,
}

/// Name of a user: the real name (with the first name) when known, the
/// login otherwise; with the id when it is empty or ids are visible.
pub fn format_user_name(
    id: i32,
    login: Option<&str>,
    realname: Option<&str>,
    firstname: Option<&str>,
    display: NameDisplay,
) -> String {
    let realname = realname.unwrap_or_default();
    let firstname = firstname.unwrap_or_default();
    let formatted = if realname.is_empty() {
        login.unwrap_or_default().to_owned()
    } else if firstname.is_empty() {
        realname.to_owned()
    } else {
        match display.format {
            NamesFormat::RealnameFirst => format!("{realname} {firstname}"),
            NamesFormat::FirstnameFirst => format!("{firstname} {realname}"),
        }
    };
    if id > 0 && (formatted.is_empty() || display.show_ids) {
        format!("{formatted} ({id})")
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(format: NamesFormat, show_ids: bool) -> NameDisplay {
        NameDisplay { format, show_ids }
    }

    #[test]
    fn real_name_with_first_name() {
        let default = NameDisplay::default();
        assert_eq!(
            format_user_name(5, Some("jdoe"), Some("Doe"), Some("John"), default),
            "Doe John"
        );
        assert_eq!(
            format_user_name(
                5,
                Some("jdoe"),
                Some("Doe"),
                Some("John"),
                display(NamesFormat::FirstnameFirst, false)
            ),
            "John Doe"
        );
        assert_eq!(
            format_user_name(5, Some("jdoe"), Some("Doe"), None, default),
            "Doe"
        );
    }

    #[test]
    fn login_when_no_real_name() {
        // Like the PHP code, a first name alone is not used.
        assert_eq!(
            format_user_name(5, Some("jdoe"), None, Some("John"), NameDisplay::default()),
            "jdoe"
        );
    }

    #[test]
    fn id_when_empty_or_requested() {
        assert_eq!(
            format_user_name(5, None, None, None, NameDisplay::default()),
            " (5)"
        );
        assert_eq!(
            format_user_name(
                5,
                Some("jdoe"),
                None,
                None,
                display(NamesFormat::RealnameFirst, true)
            ),
            "jdoe (5)"
        );
        assert_eq!(
            format_user_name(0, None, None, None, NameDisplay::default()),
            ""
        );
    }
}
