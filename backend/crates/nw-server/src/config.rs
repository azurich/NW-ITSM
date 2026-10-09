//! Runtime configuration, read from `NW_*` environment variables.

use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, bail};

#[derive(Debug, Clone)]
pub struct Config {
    /// `mysql://user:password@host:port/database`, the legacy ITSM-NG database.
    pub database_url: String,
    pub database_max_connections: u32,
    pub bind_address: SocketAddr,
    /// Key signing session tokens. At least 32 bytes.
    pub session_secret: Vec<u8>,
    pub session_ttl: Duration,
    /// Adds the `Secure` attribute to the session cookie (HTTPS only).
    pub cookie_secure: bool,
    /// Built web application (`frontend/dist`) served under `/app`.
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let session_secret = required("NW_SESSION_SECRET")?.into_bytes();
        if session_secret.len() < 32 {
            bail!("NW_SESSION_SECRET must be at least 32 bytes long");
        }
        Ok(Self {
            database_url: required("NW_DATABASE_URL")?,
            database_max_connections: parsed("NW_DATABASE_MAX_CONNECTIONS", 10)?,
            bind_address: parsed("NW_BIND_ADDRESS", SocketAddr::from(([127, 0, 0, 1], 8080)))?,
            session_secret,
            session_ttl: Duration::from_secs(60 * parsed("NW_SESSION_TTL_MINUTES", 480u64)?),
            cookie_secure: parsed("NW_COOKIE_SECURE", true)?,
            static_dir: env::var_os("NW_STATIC_DIR").map(PathBuf::from),
        })
    }
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).with_context(|| format!("environment variable {name} is required"))
}

fn parsed<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .with_context(|| format!("invalid value for {name}: {value:?}")),
        Err(_) => Ok(default),
    }
}
