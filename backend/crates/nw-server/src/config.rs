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
    /// Behind a reverse proxy: the client address is the last entry of
    /// `X-Forwarded-For`. Only enable it when a proxy always sets it.
    pub trust_proxy: bool,
    /// Key shared with the PHP application (`config/nw-bridge.key`) to log
    /// in from one interface to the other. At least 32 bytes.
    pub bridge_secret: Option<Vec<u8>>,
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
            trust_proxy: parsed("NW_TRUST_PROXY", false)?,
            bridge_secret: bridge_secret()?,
        })
    }
}

/// `NW_BRIDGE_SECRET_FILE` (usually the `config/nw-bridge.key` file of the
/// PHP application) or `NW_BRIDGE_SECRET`; none disables the bridge.
fn bridge_secret() -> anyhow::Result<Option<Vec<u8>>> {
    let secret = if let Some(path) = env::var_os("NW_BRIDGE_SECRET_FILE") {
        let content = std::fs::read(&path).with_context(|| {
            format!(
                "cannot read NW_BRIDGE_SECRET_FILE {}",
                path.to_string_lossy()
            )
        })?;
        Some(content.trim_ascii().to_vec())
    } else {
        env::var("NW_BRIDGE_SECRET").ok().map(String::into_bytes)
    };
    match secret {
        Some(secret) if secret.len() < 32 => {
            bail!("the bridge secret must be at least 32 bytes long")
        }
        secret => Ok(secret),
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
