//! Production qualification seams shared by component-specific fault routes.

pub mod blob_corruption;
#[cfg(not(verus_only))]
pub mod daemon_lifecycle;
#[cfg(not(verus_only))]
pub mod dependency;
pub mod disk;
pub mod journal;
pub mod journal_corruption;
pub mod projection;

pub use journal::{
    acquire_instance, journal_error, open_journal, verify_empty_journal,
    verify_empty_journal_for_store,
};
