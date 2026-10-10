//! Business rules shared by every NW-ITSM component.
//!
//! This crate performs no I/O: it receives plain data loaded by `nw-db`
//! and reproduces the semantics of the legacy ITSM-NG PHP code so both
//! stacks can run side by side on the same database.

pub mod asset;
pub mod crypto;
pub mod entity;
pub mod fk;
pub mod history;
pub mod i18n;
pub mod infocom;
pub mod languages;
pub mod naming;
pub mod password;
pub mod php;
pub mod purge;
pub mod rights;
pub mod rules;
pub mod template;
pub mod text;
pub mod unicity;
