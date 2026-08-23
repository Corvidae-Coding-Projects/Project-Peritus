//! Narrow shared-file `SQLite` evidence adapter.

mod connection;
mod row;
mod schema;
mod store;

pub use store::{EvidenceStore, EvidenceStoreOptions};
