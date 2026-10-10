//! History of changes (`glpi_logs`), written like `Log::history`.

use nw_domain::history::Entry;
use sqlx::MySqlConnection;

/// Records a history entry. Use the connection of the transaction that
/// changes the item.
pub async fn record(
    conn: &mut MySqlConnection,
    actor: &str,
    entry: &Entry,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO glpi_logs \
           (itemtype, items_id, itemtype_link, linked_action, user_name, date_mod, \
            id_search_option, old_value, new_value) \
         VALUES (?, ?, ?, ?, ?, NOW(), ?, ?, ?)",
    )
    .bind(&entry.itemtype)
    .bind(entry.items_id)
    .bind(&entry.itemtype_link)
    .bind(entry.action.code())
    .bind(actor)
    .bind(entry.change.search_option)
    .bind(&entry.change.old)
    .bind(&entry.change.new)
    .execute(conn)
    .await
    .map(|_| ())
}
