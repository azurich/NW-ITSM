//! Event log (`glpi_events`), written like `Event::log`.

use sqlx::MySqlPool;

/// One event of the administration log.
#[derive(Debug, Clone)]
pub struct Event<'a> {
    pub items_id: i32,
    /// Legacy `type` column: `system`, `users`...
    pub kind: &'a str,
    /// `login`, `setup`...
    pub service: &'a str,
    /// 1 (critical) to 5 (detail), as in the PHP code.
    pub level: i32,
    pub message: &'a str,
}

pub async fn log(pool: &MySqlPool, event: &Event<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO glpi_events (items_id, type, date, service, level, message) \
         VALUES (?, ?, NOW(), ?, ?, ?)",
    )
    .bind(event.items_id)
    .bind(event.kind)
    .bind(event.service)
    .bind(event.level)
    .bind(event.message)
    .execute(pool)
    .await
    .map(|_| ())
}
