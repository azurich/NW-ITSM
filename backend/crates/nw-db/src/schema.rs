//! Structure of the legacy tables.

use sqlx::{MySql, MySqlConnection, QueryBuilder};

/// Columns of a table, in their order.
pub async fn columns(conn: &mut MySqlConnection, table: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT CAST(COLUMN_NAME AS CHAR) FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
    )
    .bind(table)
    .fetch_all(conn)
    .await
}

/// Copies a row (`CommonDBTM::clone`): the given columns set, the dates of
/// creation and change (where the table has them) at the current time.
/// Returns the id of the copy.
pub async fn copy_row(
    conn: &mut MySqlConnection,
    table: &str,
    source: i32,
    set: &[(&str, i32)],
) -> Result<i32, sqlx::Error> {
    let columns = columns(conn, table).await?;
    let dates: Vec<&str> = ["date_creation", "date_mod"]
        .into_iter()
        .filter(|date| columns.iter().any(|column| column == date))
        .collect();
    let copied: Vec<String> = columns
        .iter()
        .filter(|column| {
            column.as_str() != "id"
                && !dates.contains(&column.as_str())
                && !set.iter().any(|(name, _)| name == column)
        })
        .map(|column| format!("`{column}`"))
        .collect();
    let copied = copied.join(", ");
    let mut sql = QueryBuilder::<MySql>::new(format!("INSERT INTO {table} ({copied}"));
    for column in set.iter().map(|(column, _)| column).chain(&dates) {
        sql.push(format_args!(", `{column}`"));
    }
    sql.push(format_args!(") SELECT {copied}"));
    for (_, value) in set {
        sql.push(", ").push_bind(*value);
    }
    for _ in &dates {
        sql.push(", NOW()");
    }
    sql.push(format_args!(" FROM {table} WHERE id = "));
    sql.push_bind(source);
    let result = sql.build().execute(conn).await?;
    i32::try_from(result.last_insert_id()).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}
