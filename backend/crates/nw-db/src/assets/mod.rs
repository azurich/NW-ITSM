//! Assets: computers, monitors, printers, network equipment, peripherals
//! and phones (`nw_domain::asset`).

mod purge;
pub mod racks;
mod read;
mod template;
mod write;

pub use purge::{purge, purge_blockers};
pub use read::{
    Detail, ListItem, ListQuery, Sort, Specific, TemplateSummary, get, list, list_templates,
    row_texts, specific,
};
pub use template::template_blockers;
pub use write::{
    ConnectedItem, Current, Origin, UpdateEffects, UpdateOutcome, connected, create, current,
    entity_mismatches, references_exist, set_trashed, unshare_blockers, update,
};
