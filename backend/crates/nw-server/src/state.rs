//! State shared by every request.

use std::sync::Arc;

use nw_db::MySqlPool;

use crate::{bridge::BridgeKeys, config::Config, session::SessionKeys, throttle::PasswordGuard};

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub keys: Arc<SessionKeys>,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
    pub password_guard: Arc<PasswordGuard>,
    /// `None` when the bridge with the PHP application is not configured.
    pub bridge: Option<Arc<BridgeKeys>>,
}

impl AppState {
    pub fn new(pool: MySqlPool, config: &Config) -> Self {
        Self {
            pool,
            keys: Arc::new(SessionKeys::new(&config.session_secret, config.session_ttl)),
            cookie_secure: config.cookie_secure,
            trust_proxy: config.trust_proxy,
            password_guard: Arc::new(PasswordGuard::default()),
            bridge: config
                .bridge_secret
                .as_deref()
                .map(|secret| Arc::new(BridgeKeys::new(secret))),
        }
    }
}
