//! Deterministic startup validation and shadow rebuild of every built-in projection.

use std::path::Path;

use peritus_journal::{IntegrityExport, SqliteJournal};
use peritus_projection::{
    AgentProjection, ArtifactReferenceProjection, AuthorityProjection, BudgetProjection,
    EvidenceCatalogProjection, JournalCatalogProjection, LifecycleProjection, Projection,
    ProjectionStore, RepairAction, StoreOptions, rebuild_from_genesis,
};
use peritus_trace::TraceProjection;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub fn ensure_current(
    journal: &mut SqliteJournal,
    database: &Path,
) -> Result<ProjectionStore, DaemonError> {
    let export = journal.integrity_export().map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::ReadOnly,
            "export journal for projections",
            error.to_string(),
            error,
        )
    })?;
    let mut store =
        ProjectionStore::open(database, StoreOptions::default()).map_err(projection_error)?;
    ensure(&mut store, &export, &LifecycleProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &BudgetProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &AuthorityProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &JournalCatalogProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &ArtifactReferenceProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &EvidenceCatalogProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &AgentProjection::new().map_err(projection_error)?)?;
    ensure(&mut store, &export, &TraceProjection::new().map_err(projection_error)?)?;
    Ok(store)
}

fn ensure<P: Projection>(
    store: &mut ProjectionStore,
    export: &IntegrityExport,
    projection: &P,
) -> Result<(), DaemonError> {
    match store.plan_startup(projection.schema(), export.report()).map_err(projection_error)? {
        RepairAction::Reuse(_) => Ok(()),
        RepairAction::RebuildFromGenesis(_) => {
            let expected = store
                .load_active(projection.schema())
                .map_err(projection_error)?
                .map(|active| active.generation());
            let candidate = rebuild_from_genesis(projection, export).map_err(projection_error)?;
            store.install_shadow(&candidate, expected).map(|_| ()).map_err(projection_error)
        }
    }
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
