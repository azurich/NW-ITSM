//! Limits password guessing: after too many failures, a key (login and
//! address, or address alone) is blocked until its window ends.
//!
//! The counters are kept in memory: with several API instances, each one
//! applies the limits on its own.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Prune expired windows once the map grows beyond this size.
const PRUNE_THRESHOLD: usize = 10_000;

#[derive(Debug)]
struct Window {
    started: Instant,
    failures: u32,
}

/// Failure counter over a fixed time window.
#[derive(Debug)]
pub struct Throttle {
    max_failures: u32,
    window: Duration,
    windows: Mutex<HashMap<String, Window>>,
}

impl Throttle {
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self {
            max_failures,
            window,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// `Err(retry_after)` when the key is blocked.
    pub fn check(&self, key: &str, now: Instant) -> Result<(), Duration> {
        let windows = self.windows.lock().expect("throttle lock poisoned");
        match windows.get(key) {
            Some(window) if window.failures >= self.max_failures => {
                let ends = window.started + self.window;
                if ends > now { Err(ends - now) } else { Ok(()) }
            }
            _ => Ok(()),
        }
    }

    pub fn record_failure(&self, key: &str, now: Instant) {
        let mut windows = self.windows.lock().expect("throttle lock poisoned");
        if windows.len() > PRUNE_THRESHOLD {
            windows.retain(|_, window| window.started + self.window > now);
        }
        let window = windows.entry(key.to_owned()).or_insert(Window {
            started: now,
            failures: 0,
        });
        if window.started + self.window <= now {
            *window = Window {
                started: now,
                failures: 0,
            };
        }
        window.failures += 1;
    }

    pub fn reset(&self, key: &str) {
        self.windows
            .lock()
            .expect("throttle lock poisoned")
            .remove(key);
    }
}

/// Limits applied to everything that checks a password.
#[derive(Debug)]
pub struct PasswordGuard {
    /// Per login (or user) and address.
    per_account: Throttle,
    /// Per address, whatever the login.
    per_address: Throttle,
}

impl Default for PasswordGuard {
    fn default() -> Self {
        let window = Duration::from_secs(15 * 60);
        Self {
            per_account: Throttle::new(5, window),
            per_address: Throttle::new(20, window),
        }
    }
}

impl PasswordGuard {
    /// `Err(retry_after)` when the account or the address is blocked.
    pub fn check(&self, account: &str, address: &str, now: Instant) -> Result<(), Duration> {
        self.per_address.check(address, now)?;
        self.per_account.check(&account_key(account, address), now)
    }

    pub fn failure(&self, account: &str, address: &str, now: Instant) {
        self.per_address.record_failure(address, now);
        self.per_account
            .record_failure(&account_key(account, address), now);
    }

    pub fn success(&self, account: &str, address: &str) {
        self.per_account.reset(&account_key(account, address));
    }
}

fn account_key(account: &str, address: &str) -> String {
    format!("{}\u{0}{address}", account.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_too_many_failures_until_the_window_ends() {
        let throttle = Throttle::new(3, Duration::from_secs(60));
        let start = Instant::now();
        for _ in 0..3 {
            assert!(throttle.check("key", start).is_ok());
            throttle.record_failure("key", start);
        }
        assert_eq!(
            throttle.check("key", start + Duration::from_secs(10)),
            Err(Duration::from_secs(50))
        );
        assert!(throttle.check("other", start).is_ok());
        assert!(
            throttle
                .check("key", start + Duration::from_secs(60))
                .is_ok()
        );

        // A new window starts after the old one ended.
        throttle.record_failure("key", start + Duration::from_secs(61));
        assert!(
            throttle
                .check("key", start + Duration::from_secs(62))
                .is_ok()
        );
    }

    #[test]
    fn reset_clears_the_failures() {
        let throttle = Throttle::new(1, Duration::from_secs(60));
        let now = Instant::now();
        throttle.record_failure("key", now);
        assert!(throttle.check("key", now).is_err());
        throttle.reset("key");
        assert!(throttle.check("key", now).is_ok());
    }

    #[test]
    fn guard_limits_accounts_and_addresses() {
        let guard = PasswordGuard::default();
        let now = Instant::now();
        for _ in 0..5 {
            guard.failure("Admin", "10.0.0.1", now);
        }
        // Logins are case-insensitive.
        assert!(guard.check("admin", "10.0.0.1", now).is_err());
        // The same account from another address is not blocked.
        assert!(guard.check("admin", "10.0.0.2", now).is_ok());
        // An address trying many accounts is.
        for i in 0..20 {
            guard.failure(&format!("user{i}"), "10.0.0.3", now);
        }
        assert!(guard.check("anyone", "10.0.0.3", now).is_err());
        guard.success("Admin", "10.0.0.1");
        assert!(guard.check("admin", "10.0.0.1", now).is_ok());
    }
}
