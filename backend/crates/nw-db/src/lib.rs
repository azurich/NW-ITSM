//! Data access on the legacy ITSM-NG database.
//!
//! The schema is owned by the PHP application during the migration: this
//! crate only reads it (and later writes it) without changing its structure.
//! Queries are built at runtime so the crate compiles without a database.

pub mod computers;
pub mod config;
pub mod entities;
pub mod events;
pub mod history;
pub mod profiles;
mod scope;
pub mod users;

use std::str::FromStr;

use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
pub use sqlx::{Error, MySqlPool};

/// Opens a connection pool. Sessions run in UTC so that `TIMESTAMP`
/// columns are decoded without a time zone shift.
pub async fn connect(url: &str, max_connections: u32) -> Result<MySqlPool, Error> {
    let options = MySqlConnectOptions::from_str(url)?.timezone(Some("+00:00".to_owned()));
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
}

/// Checks that the database answers.
pub async fn ping(pool: &MySqlPool) -> Result<(), Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}
