//! Deterministic startup validation and shadow rebuild of every built-in projection.

use std::path::Path;

use peritus_journal::{JournalCancellation, SqliteJournal};
use peritus_projection::{
    AgentProjection, ArtifactReferenceProjection, AuthorityProjection, BudgetProjection,
    EvidenceCatalogProjection, JournalCatalogProjection, LifecycleProjection, Projection,
    ProjectionStore, resume_or_rebuild,
};
use peritus_trace::TraceProjection;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub fn ensure_current(
    journal: &mut SqliteJournal,
    database: &Path,
    cancellation: &JournalCancellation,
) -> Result<ProjectionStore, DaemonError> {
    let mut store =
        ProjectionStore::open_waiting(database, cancellation).map_err(projection_error)?;
    ensure(
        &mut store,
        journal,
        &LifecycleProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &BudgetProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &AuthorityProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &JournalCatalogProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &ArtifactReferenceProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &EvidenceCatalogProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &AgentProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    ensure(
        &mut store,
        journal,
        &TraceProjection::new().map_err(projection_error)?,
        cancellation,
    )?;
    Ok(store)
}

fn ensure<P: Projection>(
    store: &mut ProjectionStore,
    journal: &mut SqliteJournal,
    projection: &P,
    cancellation: &JournalCancellation,
) -> Result<(), DaemonError> {
    resume_or_rebuild(store, journal, projection, cancellation)
        .map(|_| ())
        .map_err(projection_error)
}

fn projection_error(error: peritus_projection::ProjectionError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        error.operation(),
        error.to_string(),
        error,
    )
}
