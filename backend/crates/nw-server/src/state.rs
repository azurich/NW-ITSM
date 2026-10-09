//! State shared by every request.

use std::sync::Arc;

use nw_db::MySqlPool;

use crate::{config::Config, session::SessionKeys};

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub keys: Arc<SessionKeys>,
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(pool: MySqlPool, config: &Config) -> Self {
        Self {
            pool,
            keys: Arc::new(SessionKeys::new(&config.session_secret, config.session_ttl)),
            cookie_secure: config.cookie_secure,
        }
    }
}
