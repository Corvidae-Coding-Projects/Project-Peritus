//! Pure bounded selection engine.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CausalClosure, DebuggerError, DebuggerErrorKind, DebuggerLimit, DebuggerOperation,
    DebuggerRecovery, TraceSelectionQuery,
};
use peritus_journal::IntegrityExport;
use peritus_trace::{ProjectedObservation, TraceProjectionState};
use peritus_types::EventId;

use super::{
    SelectedArtifact, SelectedEvidence, SelectionCounts, TraceSelectionManifest,
    provenance::{artifact_is_committed, corruption, record_index, validate_projection_row},
};

/// Selects immutable redacted C7 evidence after checking every entry against a complete C0 export.
///
/// `ordinary_artifacts` must already be verified from finalized C0 metadata. This function never
/// reads vault bytes or mutates traces, journal state, artifacts, harnesses, or workspaces.
///
/// # Errors
///
/// Fails atomically on incomplete bindings, C7/C0 disagreement, foreign causal ancestors,
/// uncommitted ordinary artifacts, duplicate evidence, or any configured bound excess.
pub fn select_evidence(
    query: &TraceSelectionQuery,
    traces: &TraceProjectionState,
    journal: &IntegrityExport,
    ordinary_artifacts: Vec<SelectedArtifact>,
) -> Result<TraceSelectionManifest, DebuggerError> {
    let records = record_index(journal)?;
    let mut candidates =
        BTreeMap::<EventId, (&ProjectedObservation, crate::AnalysisSubject)>::new();
    for trace in traces.traces() {
        for projected in trace.observations() {
            let observation = projected.observation();
            let binding = observation.binding();
            let subject = query.subject_for_binding(binding);
            if subject.is_none()
                && query
                    .subjects()
                    .iter()
                    .any(|candidate| candidate.session_id() == binding.session_id())
                && (binding.run_id().is_none() || binding.attempt_id().is_none())
            {
                return Err(selection_error(
                    "production selection encountered an incomplete run/attempt binding",
                ));
            }
            let Some(subject) = subject else { continue };
            let record = records
                .get(&observation.event_id())
                .ok_or_else(|| corruption("selected C7 event is absent from the C0 export"))?;
            validate_projection_row(projected, record)?;
            if candidates.insert(observation.event_id(), (projected, subject.clone())).is_some() {
                return Err(corruption("selected C7 event identity is duplicated"));
            }
        }
    }

    let mut selected: BTreeSet<EventId> = candidates
        .iter()
        .filter_map(|(event, (projected, _))| {
            query.directly_matches(projected.observation()).then_some(*event)
        })
        .collect();
    if query.causal_closure() == CausalClosure::IncludeAncestors {
        close_ancestors(&candidates, &mut selected, query)?;
    }

    let mut entries = Vec::with_capacity(selected.len());
    for event_id in selected {
        let (projected, subject) = candidates
            .get(&event_id)
            .ok_or_else(|| corruption("causal closure selected an absent event"))?;
        let frame_length = u64::try_from(projected.frame_bytes().len())
            .map_err(|_| selection_error("trace frame length cannot be represented"))?;
        entries.push(SelectedEvidence::checked(
            subject.clone(),
            projected.observation(),
            projected.journal_position(),
            projected.frame_digest(),
            frame_length,
        ));
    }
    entries.sort_by_key(|entry| (entry.subject().id(), entry.journal_position(), entry.event_id()));

    let limits = query.limits();
    limits.check(DebuggerLimit::Events, entries.len(), DebuggerOperation::SelectEvidence)?;
    let trace_ids: BTreeSet<_> = entries.iter().map(SelectedEvidence::trace_id).collect();
    limits.check(DebuggerLimit::Traces, trace_ids.len(), DebuggerOperation::SelectEvidence)?;
    let causal_edges = entries.iter().try_fold(0_usize, |total, entry| {
        total
            .checked_add(entry.causal_events().len())
            .ok_or_else(|| selection_error("causal-edge count overflow"))
    })?;
    limits.check(DebuggerLimit::CausalEdges, causal_edges, DebuggerOperation::SelectEvidence)?;

    let artifacts = validate_artifacts(ordinary_artifacts, &entries, journal, query)?;
    let artifact_bytes = artifacts.iter().try_fold(0_u64, |total, artifact| {
        total
            .checked_add(artifact.size())
            .ok_or_else(|| selection_error("artifact byte count overflow"))
    })?;
    if artifact_bytes > limits.get(DebuggerLimit::ArtifactBytes) {
        return Err(DebuggerError::numbers(
            DebuggerErrorKind::Budget,
            DebuggerOperation::SelectEvidence,
            DebuggerRecovery::CorrectInput,
            "selected artifact bytes exceed the job ceiling",
            limits.get(DebuggerLimit::ArtifactBytes),
            artifact_bytes,
        ));
    }
    let counts = SelectionCounts::new(
        u64::try_from(query.subjects().len())
            .map_err(|_| selection_error("subject count overflow"))?,
        u64::try_from(trace_ids.len()).map_err(|_| selection_error("trace count overflow"))?,
        u64::try_from(entries.len()).map_err(|_| selection_error("event count overflow"))?,
        u64::try_from(causal_edges).map_err(|_| selection_error("causal count overflow"))?,
        u64::try_from(artifacts.len()).map_err(|_| selection_error("artifact count overflow"))?,
        artifact_bytes,
    );
    TraceSelectionManifest::checked(query, entries, artifacts, counts)
}

fn close_ancestors(
    candidates: &BTreeMap<EventId, (&ProjectedObservation, crate::AnalysisSubject)>,
    selected: &mut BTreeSet<EventId>,
    query: &TraceSelectionQuery,
) -> Result<(), DebuggerError> {
    let mut frontier: Vec<EventId> = selected.iter().copied().collect();
    while let Some(event_id) = frontier.pop() {
        let (projected, subject) = candidates
            .get(&event_id)
            .ok_or_else(|| corruption("causal closure frontier is absent"))?;
        for cause in projected.observation().causal_events() {
            let (ancestor, owner) = candidates
                .get(cause)
                .ok_or_else(|| corruption("causal ancestor is not a selectable C0-backed event"))?;
            if owner.id() != subject.id()
                || ancestor.observation().binding().session_id() != subject.session_id()
            {
                return Err(corruption("causal closure crosses an analysis subject"));
            }
            if selected.insert(*cause) {
                query.limits().check(
                    DebuggerLimit::Events,
                    selected.len(),
                    DebuggerOperation::SelectEvidence,
                )?;
                frontier.push(*cause);
            }
        }
    }
    Ok(())
}

fn validate_artifacts(
    mut artifacts: Vec<SelectedArtifact>,
    entries: &[SelectedEvidence],
    export: &IntegrityExport,
    query: &TraceSelectionQuery,
) -> Result<Vec<SelectedArtifact>, DebuggerError> {
    artifacts.sort_by_key(SelectedArtifact::digest);
    if artifacts.windows(2).any(|pair| pair[0].digest() == pair[1].digest()) {
        return Err(selection_error("ordinary artifact inventory repeats a digest"));
    }
    query.limits().check(
        DebuggerLimit::ArtifactCitations,
        artifacts.len(),
        DebuggerOperation::SelectEvidence,
    )?;
    let positions: BTreeMap<_, _> =
        entries.iter().map(|entry| (entry.event_id(), entry.journal_position())).collect();
    for artifact in &artifacts {
        if artifact.source_event().is_some_and(|event| !positions.contains_key(&event)) {
            return Err(selection_error("event-scoped artifact names an unselected event"));
        }
        if !artifact_is_committed(export, artifact, &positions) {
            return Err(corruption("selected ordinary artifact is absent from C0 references"));
        }
    }
    Ok(artifacts)
}

fn selection_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Selection,
        DebuggerOperation::SelectEvidence,
        DebuggerRecovery::RepairDependency,
        detail,
    )
}
