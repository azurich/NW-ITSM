//! Password verification compatible with the hashes stored by ITSM-NG.
//!
//! Mirrors `Auth::checkPassword`: bcrypt for current accounts, and the
//! unsalted MD5 / SHA1 and salted SHA1 formats left by very old versions.

use chrono::{DateTime, TimeDelta, Utc};
use md5::Md5;
use sha1::{Digest, Sha1};
use subtle::ConstantTimeEq;

/// Bcrypt hash of a throwaway secret, verified when the login is unknown
/// so that the response time does not reveal which accounts exist.
const TIMING_EQUALIZER_HASH: &str = "$2y$10$jigpcJh.Sf/ip1FiBh16l.UXVVcMDecYdaDjSdwgoKV2OBO4n46hi";

/// Checks a plain-text password against a stored hash.
///
/// Unknown hash formats (including empty hashes) never match.
pub fn verify(password: &str, hash: &str) -> bool {
    if is_bcrypt(hash) {
        return bcrypt::verify(password, hash).unwrap_or(false);
    }
    if hash.starts_with('$') {
        // Another `password_hash` algorithm (argon2...). ITSM-NG always
        // uses PASSWORD_DEFAULT, so such a hash is unexpected: refuse it.
        return false;
    }
    match hash.len() {
        32 => constant_time_eq(&hex::encode(Md5::digest(password)), hash),
        40 => constant_time_eq(&hex::encode(Sha1::digest(password)), hash),
        48 if hash.is_char_boundary(8) => {
            let (salt, _) = hash.split_at(8);
            let digest = hex::encode(Sha1::digest(format!("{salt}{password}")));
            constant_time_eq(&format!("{salt}{digest}"), hash)
        }
        _ => false,
    }
}

/// Spends the same time as a real bcrypt verification.
pub fn equalize_timing(password: &str) {
    let _ = bcrypt::verify(password, TIMING_EQUALIZER_HASH);
}

/// Outcome of the password expiration policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiration {
    Valid,
    /// The password must be changed before using the application.
    Expired,
    /// Expired for longer than the lock delay: the legacy code disables
    /// the account.
    Locked,
}

/// Applies the policy of `Auth::connection_db`. Delays are in days and
/// `-1` disables the policy. Like the legacy code, a password without a
/// last update date is considered expired when the policy is enabled.
pub fn expiration(
    last_update: Option<DateTime<Utc>>,
    delay_days: i64,
    lock_delay_days: i64,
    now: DateTime<Utc>,
) -> Expiration {
    if delay_days == -1 {
        return Expiration::Valid;
    }
    let older_than = |days: i64| last_update.is_none_or(|date| date + TimeDelta::days(days) < now);
    if lock_delay_days != -1 && older_than(delay_days + lock_delay_days) {
        Expiration::Locked
    } else if older_than(delay_days) {
        Expiration::Expired
    } else {
        Expiration::Valid
    }
}

fn is_bcrypt(hash: &str) -> bool {
    ["$2y$", "$2a$", "$2b$"]
        .iter()
        .any(|prefix| hash.starts_with(prefix))
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcrypt_hash_from_php() {
        // password_hash('itsm', PASSWORD_DEFAULT) from a fresh install.
        let hash = "$2y$10$w8wX.GPGo5q5tlyIXIzY/eBAk/a2r8KmHY8SIl32Q./1Na/nKEkUy";
        assert!(verify("itsm", hash));
        assert!(!verify("wrong", hash));
    }

    #[test]
    fn legacy_md5() {
        assert!(verify("secret", "5ebe2294ecd0e0f08eab7690d2a6ee69"));
        assert!(!verify("other", "5ebe2294ecd0e0f08eab7690d2a6ee69"));
    }

    #[test]
    fn legacy_sha1() {
        assert!(verify("secret", "e5e9fa1ba31ecd1ae84f75caaa474f3a663f05f4"));
        assert!(!verify("other", "e5e9fa1ba31ecd1ae84f75caaa474f3a663f05f4"));
    }

    #[test]
    fn legacy_salted_sha1() {
        let hash = "abcdefgh61bff3a00177e2c11fa0b284afc956a1342794e6";
        assert!(verify("secret", hash));
        assert!(!verify("other", hash));
    }

    #[test]
    fn unknown_formats_never_match() {
        assert!(!verify("", ""));
        assert!(!verify("secret", "not-a-hash"));
        assert!(!verify(
            "secret",
            "$argon2id$v=19$m=65536,t=4,p=1$c2FsdA$aGFzaA"
        ));
    }

    #[test]
    fn expiration_policy() {
        let now = Utc::now();
        let ago = |days| Some(now - TimeDelta::days(days));
        assert_eq!(expiration(None, -1, -1, now), Expiration::Valid);
        assert_eq!(expiration(ago(10), 30, -1, now), Expiration::Valid);
        assert_eq!(expiration(ago(31), 30, -1, now), Expiration::Expired);
        assert_eq!(expiration(ago(31), 30, 10, now), Expiration::Expired);
        assert_eq!(expiration(ago(41), 30, 10, now), Expiration::Locked);
        assert_eq!(expiration(None, 30, -1, now), Expiration::Expired);
        assert_eq!(expiration(None, 30, 10, now), Expiration::Locked);
    }

    #[test]
    fn timing_equalizer_hash_is_valid_bcrypt() {
        assert!(verify("nw-itsm-timing-equalizer", TIMING_EQUALIZER_HASH));
    }
}
