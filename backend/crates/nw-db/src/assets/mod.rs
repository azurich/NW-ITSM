//! Assets: computers, monitors, printers, network equipment, peripherals
//! and phones (`nw_domain::asset`).

mod read;
mod write;

pub use read::{Detail, ListItem, ListQuery, Sort, get, list};
pub use write::{
    Current, UpdateEffects, UpdateOutcome, connected_kinds, create, current, forwarded_items,
    references_exist, set_trashed, update,
};
