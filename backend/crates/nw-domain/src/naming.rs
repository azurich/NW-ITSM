//! Display names.

/// Name shown for a user: "Lastname Firstname" when known (legacy default
/// `names_format`), the login otherwise.
pub fn user_display_name(
    login: Option<&str>,
    realname: Option<&str>,
    firstname: Option<&str>,
) -> String {
    let full = [realname, firstname]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if full.is_empty() {
        login.unwrap_or_default().to_owned()
    } else {
        full
    }
}

#[cfg(test)]
mod tests {
    use super::user_display_name;

    #[test]
    fn prefers_real_names() {
        assert_eq!(
            user_display_name(Some("jdoe"), Some("Doe"), Some("John")),
            "Doe John"
        );
        assert_eq!(user_display_name(Some("jdoe"), Some("Doe"), None), "Doe");
        assert_eq!(user_display_name(Some("jdoe"), Some(" "), Some("")), "jdoe");
        assert_eq!(user_display_name(None, None, None), "");
    }
}
