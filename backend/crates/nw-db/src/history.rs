//! History of changes (`glpi_logs`), written like `Log::history`.

use nw_domain::history::Change;
use sqlx::MySqlConnection;

/// Records a field change of an item. Use the connection of the
/// transaction that changes the item.
pub async fn record(
    conn: &mut MySqlConnection,
    itemtype: &str,
    items_id: i32,
    actor: &str,
    change: &Change,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO glpi_logs \
           (itemtype, items_id, itemtype_link, linked_action, user_name, date_mod, \
            id_search_option, old_value, new_value) \
         VALUES (?, ?, '', 0, ?, NOW(), ?, ?, ?)",
    )
    .bind(itemtype)
    .bind(items_id)
    .bind(actor)
    .bind(change.search_option)
    .bind(&change.old)
    .bind(&change.new)
    .execute(conn)
    .await
    .map(|_| ())
}
