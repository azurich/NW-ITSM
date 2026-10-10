//! PHP comparison semantics the legacy code relies on (PHP 8): loose
//! equality of strings, `empty()`, `trim()` and case-insensitive search.

/// Number of a PHP numeric string (`is_numeric`): optional surrounding
/// whitespace, sign, digits with an optional fraction and exponent.
pub fn numeric(value: &str) -> Option<f64> {
    let trimmed =
        value.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C'));
    let body = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let has_digits = !integer.is_empty() || fraction.is_some_and(|fraction| !fraction.is_empty());
    if !has_digits || !digits(integer) || !fraction.is_none_or(digits) {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !digits(exponent) {
            return None;
        }
    }
    trimmed.parse::<f64>().ok()
}

/// `$a == $b` for two strings: numeric strings are compared as numbers.
pub fn loose_eq(a: &str, b: &str) -> bool {
    match (numeric(a), numeric(b)) {
        (Some(x), Some(y)) => match (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
            (Ok(x), Ok(y)) => x == y,
            _ => x == y,
        },
        _ => a == b,
    }
}

/// `empty($value)` for a string.
pub fn is_empty(value: &str) -> bool {
    value.is_empty() || value == "0"
}

/// `trim()` with its default characters.
pub fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
}

/// `mb_strtolower`.
pub fn lower(value: &str) -> String {
    value.to_lowercase()
}

/// Whether `mb_stripos($haystack, $needle)` finds the needle, and at the
/// start when `at_start`.
pub fn contains_ci(haystack: &str, needle: &str, at_start: bool) -> bool {
    let (haystack, needle) = (lower(haystack), lower(needle));
    if at_start {
        haystack.starts_with(&needle)
    } else {
        haystack.contains(&needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_strings() {
        for value in ["1", " 1", "1 ", "-1", "+1.5", ".5", "5.", "1e3", "1E-3"] {
            assert!(numeric(value).is_some(), "{value}");
        }
        for value in ["", " ", "abc", "1a", "1e", "--1", ".", "0x1A", "1 2"] {
            assert!(numeric(value).is_none(), "{value}");
        }
    }

    #[test]
    fn loose_equality_of_php_8() {
        assert!(loose_eq("1", "01"));
        assert!(loose_eq("1", "1.0"));
        assert!(loose_eq("abc", "abc"));
        assert!(!loose_eq("", "0"));
        assert!(!loose_eq("abc", "ABC"));
        assert!(!loose_eq("1", "1a"));
    }

    #[test]
    fn emptiness_and_search() {
        assert!(is_empty("") && is_empty("0"));
        assert!(!is_empty("00") && !is_empty(" "));
        assert_eq!(trim(" \tx\n"), "x");
        assert!(contains_ci("Bureau Siège", "SIÈGE", false));
        assert!(contains_ci("PC-01", "pc", true));
        assert!(!contains_ci("PC-01", "01", true));
    }
}
