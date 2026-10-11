//! Narrow shared-file `SQLite` evidence adapter.

mod connection;
mod containment;
mod quarantine;
mod row;
mod schema;
mod store;

pub use containment::EvidenceContainmentProgress;
pub use quarantine::{EvidenceQuarantine, EvidenceQuarantineAudit, EvidenceQuarantineId};
pub use store::{EvidenceStore, EvidenceStoreOptions};
