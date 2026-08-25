//! Restart recovery and deterministic C0 rebuild entry points.

use peritus_journal::{AggregateHead, AggregateId, AggregateKey, AggregateKind, SqliteJournal};
use peritus_projection::replay_from_genesis;

use crate::{TraceError, TraceId, TraceProjection, TraceProjectionState, TraceSnapshot};

/// Rebuilt state and exact durable head for one trace aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredTrace {
    trace: Option<TraceSnapshot>,
    head: Option<AggregateHead>,
}

impl RecoveredTrace {
    /// Borrows the rebuilt trace, or `None` when the aggregate is absent.
    #[must_use]
    pub const fn trace(&self) -> Option<&TraceSnapshot> {
        self.trace.as_ref()
    }
    /// Returns the exact C0 aggregate head, or absence.
    #[must_use]
    pub const fn head(&self) -> Option<AggregateHead> {
        self.head
    }
}

/// Rebuilds one trace from its complete checked aggregate chain.
///
/// # Errors
///
/// Returns C0 integrity, canonical frame, or causal projection failures.
pub fn recover_trace(
    journal: &SqliteJournal,
    trace_id: TraceId,
) -> Result<RecoveredTrace, TraceError> {
    let id = AggregateId::new(trace_id.into_bytes())
        .map_err(|error| TraceError::journal("derive recovery aggregate", &error))?;
    let key = AggregateKey::new(AggregateKind::Trace, id);
    let head = journal
        .head(key)
        .map_err(|error| TraceError::journal("read recovery trace head", &error))?;
    let records = journal
        .records_for_aggregate(key)
        .map_err(|error| TraceError::journal("read recovery trace history", &error))?;
    let mut state = TraceProjectionState::default();
    for record in &records {
        state.apply_record(record)?;
    }
    Ok(RecoveredTrace { trace: state.trace(trace_id).cloned(), head })
}

/// Integrity-scans the complete journal and rebuilds all trace projections from genesis.
///
/// # Errors
///
/// Returns C0 integrity or pure projection failures. Recovery performs no external execution.
pub fn recover_all(journal: &mut SqliteJournal) -> Result<TraceProjectionState, TraceError> {
    let export = journal
        .integrity_export()
        .map_err(|error| TraceError::journal("export trace recovery history", &error))?;
    let projection = TraceProjection::new()
        .map_err(|error| TraceError::projection("create trace projection", &error))?;
    replay_from_genesis(&projection, &export)
        .map(peritus_projection::ReplayOutput::into_state)
        .map_err(|error| TraceError::projection("rebuild trace projection", &error))
}
