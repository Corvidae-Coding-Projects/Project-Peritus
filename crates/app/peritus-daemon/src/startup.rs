//! Deterministic production startup composition.

mod evolution;
mod migration;
mod plan;
mod projection;
mod recovery;
mod registry;
mod runtime;
pub mod workspace;

use std::path::Path;

use peritus_journal::{JournalCancellation, SqliteJournal};
use peritus_projection::ProjectionStore;

use crate::DaemonError;

/// Rebuilds every startup projection with a standalone live cancellation owner.
pub fn ensure_projections_current(
    journal: &mut SqliteJournal,
    database: &Path,
) -> Result<ProjectionStore, DaemonError> {
    projection::ensure_current(journal, database, &JournalCancellation::new())
}
pub use runtime::DaemonRuntime;
