//! Narrow shared-file `SQLite` evidence adapter.

mod connection;
mod quarantine;
mod row;
mod schema;
mod store;

pub use quarantine::EvidenceQuarantine;
pub use store::{EvidenceStore, EvidenceStoreOptions};
