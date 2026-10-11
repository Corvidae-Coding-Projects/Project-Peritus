//! Production qualification seams shared by component-specific fault routes.

pub mod blob_corruption;

pub mod daemon_lifecycle;

pub mod dependency;
pub mod disk;
pub mod disk_journal;
pub mod evidence_corruption;
pub mod journal;
pub mod journal_corruption;
pub mod projection;
pub mod promotion_evidence_corruption;

pub use journal::{
    acquire_instance, journal_error, open_journal, verify_empty_journal,
    verify_empty_journal_for_store,
};
