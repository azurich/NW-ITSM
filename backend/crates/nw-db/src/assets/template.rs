//! Items created from a template: the linked rows of the template cloned
//! for them (`nw_domain::template`).

use nw_domain::{
    asset::AssetKind,
    history::{self, Entry},
    template::{CLONED_LINKS, ClonedLink},
};
use sqlx::{MySqlConnection, MySqlPool};

use crate::infocoms::{self, InfocomSettings, Owner};

/// Tables of the linked rows of a template whose cloning is not ported:
/// creating an item from it is then left to the classic interface.
pub async fn template_blockers(
    pool: &MySqlPool,
    kind: AssetKind,
    id: i32,
) -> Result<Vec<&'static str>, sqlx::Error> {
    super::purge::present(pool, kind, id, kind.template_blockers()).await
}

/// One side of a link, for its history.
struct Side<'a> {
    itemtype: &'a str,
    id: i32,
    name: Option<String>,
}

/// Clones the linked rows of the template for the new item, in the order
/// of the PHP code. `name_id` formats the names in the history of the
/// links (`"%1$s (%2$s)"`, translated).
pub(super) async fn clone_links(
    conn: &mut MySqlConnection,
    kind: AssetKind,
    template_id: i32,
    owner: Owner<'_>,
    name_id: &str,
    settings: &InfocomSettings,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let name: Option<String> =
        sqlx::query_scalar(&format!("SELECT name FROM {} WHERE id = ?", kind.table()))
            .bind(owner.id)
            .fetch_one(&mut *conn)
            .await?;
    let item = Side {
        itemtype: kind.itemtype(),
        id: owner.id,
        name,
    };
    for link in CLONED_LINKS {
        match link {
            ClonedLink::Infocom => {
                infocoms::clone_for_item(conn, settings, template_id, owner, actor).await?;
            }
            ClonedLink::Contract => {
                clone_contracts(conn, &item, template_id, name_id, actor).await?;
            }
            ClonedLink::Document => {
                clone_documents(conn, &item, template_id, owner, name_id, actor).await?;
            }
        }
    }
    Ok(())
}

/// `Contract_Item::clone`: refused when the contract has all the items it
/// allows (`max_links_allowed`).
async fn clone_contracts(
    conn: &mut MySqlConnection,
    item: &Side<'_>,
    template_id: i32,
    name_id: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let links: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT id, contracts_id FROM glpi_contracts_items \
         WHERE itemtype = ? AND items_id = ? ORDER BY id",
    )
    .bind(item.itemtype)
    .bind(template_id)
    .fetch_all(&mut *conn)
    .await?;
    for (source, contracts_id) in links {
        let contract: Option<(Option<String>, i32)> =
            sqlx::query_as("SELECT name, max_links_allowed FROM glpi_contracts WHERE id = ?")
                .bind(contracts_id)
                .fetch_optional(&mut *conn)
                .await?;
        if let Some((_, max)) = &contract
            && *max > 0
        {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM glpi_contracts_items WHERE contracts_id = ?",
            )
            .bind(contracts_id)
            .fetch_one(&mut *conn)
            .await?;
            if count >= i64::from(*max) {
                continue;
            }
        }
        crate::schema::copy_row(
            conn,
            "glpi_contracts_items",
            source,
            &[("items_id", item.id)],
        )
        .await?;
        if let Some((name, _)) = contract {
            let contract = Side {
                itemtype: "Contract",
                id: contracts_id,
                name,
            };
            record_link(conn, actor, name_id, &contract, item).await?;
        }
    }
    Ok(())
}

/// `Document_Item::clone`: its author, position and date kept; refused
/// when the item already has the document at this position.
async fn clone_documents(
    conn: &mut MySqlConnection,
    item: &Side<'_>,
    template_id: i32,
    owner: Owner<'_>,
    name_id: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let links: Vec<(i32, i32, i64)> = sqlx::query_as(
        "SELECT id, documents_id, CAST(timeline_position AS SIGNED) FROM glpi_documents_items \
         WHERE itemtype = ? AND items_id = ? ORDER BY id",
    )
    .bind(item.itemtype)
    .bind(template_id)
    .fetch_all(&mut *conn)
    .await?;
    for (source, documents_id, timeline_position) in links {
        if documents_id == 0 {
            continue;
        }
        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM glpi_documents_items \
             WHERE documents_id = ? AND itemtype = ? AND items_id = ? AND timeline_position = ?",
        )
        .bind(documents_id)
        .bind(item.itemtype)
        .bind(item.id)
        .bind(timeline_position)
        .fetch_one(&mut *conn)
        .await?;
        if existing > 0 {
            continue;
        }
        crate::schema::copy_row(
            conn,
            "glpi_documents_items",
            source,
            &[
                ("items_id", item.id),
                ("entities_id", owner.entities_id),
                ("is_recursive", i32::from(owner.is_recursive)),
            ],
        )
        .await?;
        let name: Option<Option<String>> =
            sqlx::query_scalar("SELECT name FROM glpi_documents WHERE id = ?")
                .bind(documents_id)
                .fetch_optional(&mut *conn)
                .await?;
        if let Some(name) = name {
            let document = Side {
                itemtype: "Document",
                id: documents_id,
                name,
            };
            record_link(conn, actor, name_id, &document, item).await?;
        }
    }
    Ok(())
}

/// History of a new link on both sides (`CommonDBRelation::post_addItem`):
/// on the first, then on the second, each naming the other.
async fn record_link(
    conn: &mut MySqlConnection,
    actor: &str,
    name_id: &str,
    first: &Side<'_>,
    second: &Side<'_>,
) -> Result<(), sqlx::Error> {
    let name = |side: &Side<'_>| history::name_id(name_id, side.name.as_deref(), side.id);
    let entries = [
        Entry::relation(first.itemtype, first.id, second.itemtype, &name(second)),
        Entry::relation(second.itemtype, second.id, first.itemtype, &name(first)),
    ];
    for entry in &entries {
        crate::history::record(conn, actor, entry).await?;
    }
    Ok(())
}
