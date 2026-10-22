//! Text as stored by the PHP application.
//!
//! Its input filter (`Toolbox::sanitize`, `clean_cross_side_scripting_deep`)
//! stores `<` and `>` as `&lt;` and `&gt;`, and the classic interface relies
//! on it when displaying values. Text written here must be stored the same
//! way, and is decoded when read.

/// Value to store, as the PHP input filter would.
pub fn to_stored(value: &str) -> String {
    value.replace('<', "&lt;").replace('>', "&gt;")
}

/// Text of a stored value (legacy `unclean_cross_side_scripting_deep`).
pub fn from_stored(value: &str) -> String {
    value.replace("&lt;", "<").replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_the_php_encoding() {
        // Stored by Computer::add with the sanitized form input.
        let stored = "A&lt;b&gt;&C 'x' \"q\"";
        assert_eq!(to_stored("A<b>&C 'x' \"q\""), stored);
        assert_eq!(from_stored(stored), "A<b>&C 'x' \"q\"");
    }
}
