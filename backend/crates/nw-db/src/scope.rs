//! SQL translation of the entity scope.

use nw_domain::entity::EntityScope;
use sqlx::{MySql, QueryBuilder};

/// Appends ` AND <entity restriction>` for a table alias, like the legacy
/// `getEntitiesRestrictCriteria($table, '', '', true)`.
///
/// `alias` must be a trusted SQL identifier, never user input.
pub(crate) fn restrict_entities(
    query: &mut QueryBuilder<'_, MySql>,
    alias: &str,
    scope: &EntityScope,
) {
    if scope.is_unrestricted() {
        return;
    }
    if scope.active().is_empty() {
        query.push(" AND 1 = 0");
        return;
    }

    query.push(format_args!(" AND ({alias}.entities_id IN ("));
    let mut active = query.separated(", ");
    for id in scope.active() {
        active.push_bind(*id);
    }
    query.push(")");

    if !scope.parents().is_empty() {
        query.push(format_args!(
            " OR ({alias}.is_recursive = 1 AND {alias}.entities_id IN ("
        ));
        let mut parents = query.separated(", ");
        for id in scope.parents() {
            parents.push_bind(*id);
        }
        query.push("))");
    }
    query.push(")");
}
