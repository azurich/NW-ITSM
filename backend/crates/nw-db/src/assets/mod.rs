//! Assets: computers, monitors, printers, network equipment, peripherals
//! and phones (`nw_domain::asset`).

mod read;
mod write;

pub use read::{Detail, ListItem, ListQuery, Sort, get, list};
pub use write::{
    ConnectedItem, Current, UpdateEffects, UpdateOutcome, connected, create, current,
    entity_mismatches, references_exist, set_trashed, unshare_blockers, update,
};
