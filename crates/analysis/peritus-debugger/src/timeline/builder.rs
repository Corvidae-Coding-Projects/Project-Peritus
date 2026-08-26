//! Canonical timeline construction and outcome normalization.

use std::collections::BTreeMap;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerLimit, DebuggerLimits, DebuggerOperation,
    DebuggerRecovery, EvidenceCitation, InfrastructureOutcome, OutcomeClass, SelectedEvidence,
    SubjectId, TaskOutcome, TraceSelectionManifest,
};
use peritus_trace::{
    DiagnosticCode, ObservationKind, SafeAttributeKey, SafeAttributeValue, SpanId, SpanKind,
    SpanOutcome,
};
use peritus_types::EventId;

/// Normalized boundary retained for deterministic analyzers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundaryKind {
    /// A span of the named kind opened.
    Started(SpanKind),
    /// One closed diagnostic occurred.
    Diagnostic(DiagnosticCode),
    /// A span reached the named terminal outcome.
    Ended(SpanOutcome),
}

/// One closed resource observation copied from safe C7 scalar attributes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceObservation {
    key: SafeAttributeKey,
    value: SafeAttributeValue,
}

impl ResourceObservation {
    /// Returns the resource dimension.
    #[must_use]
    pub const fn key(self) -> SafeAttributeKey {
        self.key
    }
    /// Returns the safe scalar value.
    #[must_use]
    pub const fn value(self) -> SafeAttributeValue {
        self.value
    }
}

/// Explicit disagreement between source wall time and deterministic order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClockAmbiguity {
    earlier: EvidenceCitation,
    later: EvidenceCitation,
    earlier_unix_nanos: u64,
    later_unix_nanos: u64,
}

impl ClockAmbiguity {
    /// Borrows the earlier deterministic-order citation.
    #[must_use]
    pub const fn earlier(&self) -> &EvidenceCitation {
        &self.earlier
    }
    /// Borrows the later deterministic-order citation.
    #[must_use]
    pub const fn later(&self) -> &EvidenceCitation {
        &self.later
    }
    /// Returns the earlier source wall time.
    #[must_use]
    pub const fn earlier_unix_nanos(&self) -> u64 {
        self.earlier_unix_nanos
    }
    /// Returns the regressed later source wall time.
    #[must_use]
    pub const fn later_unix_nanos(&self) -> u64 {
        self.later_unix_nanos
    }
}

/// One exact normalized timeline entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineEntry {
    span_id: SpanId,
    citation: EvidenceCitation,
    boundary: BoundaryKind,
    outcome: Option<OutcomeClass>,
    resources: Vec<ResourceObservation>,
    predecessor_indices: Vec<u64>,
    missing_predecessors: Vec<EventId>,
    monotonic_tick: u64,
    unix_nanos: u64,
}

impl TimelineEntry {
    /// Borrows the exact source citation.
    #[must_use]
    pub const fn citation(&self) -> &EvidenceCitation {
        &self.citation
    }
    /// Returns the exact span identity.
    #[must_use]
    pub const fn span_id(&self) -> SpanId {
        self.span_id
    }
    /// Returns the normalized boundary.
    #[must_use]
    pub const fn boundary(&self) -> BoundaryKind {
        self.boundary
    }
    /// Returns the normalized task/infrastructure outcome, when terminal or diagnostic.
    #[must_use]
    pub const fn outcome(&self) -> Option<OutcomeClass> {
        self.outcome
    }
    /// Borrows canonical resource observations.
    #[must_use]
    pub fn resources(&self) -> &[ResourceObservation] {
        &self.resources
    }
    /// Borrows indices of selected causal predecessors.
    #[must_use]
    pub fn predecessor_indices(&self) -> &[u64] {
        &self.predecessor_indices
    }
    /// Borrows causal predecessors omitted by a `SelectedOnly` query.
    #[must_use]
    pub fn missing_predecessors(&self) -> &[EventId] {
        &self.missing_predecessors
    }
    /// Returns the source monotonic tick.
    #[must_use]
    pub const fn monotonic_tick(&self) -> u64 {
        self.monotonic_tick
    }
    /// Returns the source wall time.
    #[must_use]
    pub const fn unix_nanos(&self) -> u64 {
        self.unix_nanos
    }
}

/// Complete canonical timeline for one production subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timeline {
    subject_id: SubjectId,
    entries: Vec<TimelineEntry>,
    clock_ambiguities: Vec<ClockAmbiguity>,
}

impl Timeline {
    /// Returns the exact subject identity.
    #[must_use]
    pub const fn subject_id(&self) -> SubjectId {
        self.subject_id
    }
    /// Borrows entries in deterministic `(tick, C0 position, event)` order.
    #[must_use]
    pub fn entries(&self) -> &[TimelineEntry] {
        &self.entries
    }
    /// Borrows explicit source wall-clock disagreements.
    #[must_use]
    pub fn clock_ambiguities(&self) -> &[ClockAmbiguity] {
        &self.clock_ambiguities
    }
}

/// Builds one canonical timeline for every frozen query subject.
///
/// # Errors
///
/// Rejects incomplete subject evidence, invalid citations, predecessor index overflow, or bounds.
pub fn build_timelines(
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<Vec<Timeline>, DebuggerError> {
    let mut timelines = Vec::with_capacity(manifest.subjects().len());
    let mut total_entries = 0_usize;
    for subject in manifest.subjects() {
        let mut selected: Vec<&SelectedEvidence> = manifest
            .entries()
            .iter()
            .filter(|entry| entry.subject().id() == subject.id())
            .collect();
        selected.sort_by_key(|entry| {
            (entry.time().monotonic_tick(), entry.journal_position(), entry.event_id())
        });
        total_entries = total_entries
            .checked_add(selected.len())
            .ok_or_else(|| timeline_error("timeline entry count overflow"))?;
        limits.check(
            DebuggerLimit::TimelineEntries,
            total_entries,
            DebuggerOperation::BuildTimeline,
        )?;
        timelines.push(build_one(manifest, subject.id(), &selected, limits)?);
    }
    Ok(timelines)
}

fn build_one(
    manifest: &TraceSelectionManifest,
    subject_id: SubjectId,
    selected: &[&SelectedEvidence],
    limits: DebuggerLimits,
) -> Result<Timeline, DebuggerError> {
    let positions: BTreeMap<EventId, u64> = selected
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            u64::try_from(index)
                .map(|index| (entry.event_id(), index))
                .map_err(|_| timeline_error("timeline index cannot be represented"))
        })
        .collect::<Result<_, _>>()?;
    let mut entries = Vec::with_capacity(selected.len());
    let span_kinds: BTreeMap<SpanId, SpanKind> = selected
        .iter()
        .filter_map(|entry| match entry.kind() {
            ObservationKind::SpanStarted(kind) => Some((entry.span_id(), kind)),
            _ => None,
        })
        .collect();
    for evidence in selected {
        let citation = EvidenceCitation::new(
            manifest,
            subject_id,
            evidence.event_id(),
            evidence.journal_position(),
            evidence.frame_digest(),
            None,
        )?;
        let mut predecessor_indices = Vec::new();
        let mut missing_predecessors = Vec::new();
        for event in evidence.causal_events() {
            if let Some(index) = positions.get(event) {
                predecessor_indices.push(*index);
            } else {
                missing_predecessors.push(*event);
            }
        }
        let resources = resource_observations(evidence);
        entries.push(TimelineEntry {
            span_id: evidence.span_id(),
            citation,
            boundary: boundary(evidence.kind()),
            outcome: normalize_outcome(evidence, span_kinds.get(&evidence.span_id()).copied()),
            resources,
            predecessor_indices,
            missing_predecessors,
            monotonic_tick: evidence.time().monotonic_tick(),
            unix_nanos: evidence.time().unix_nanos(),
        });
    }
    let mut clock_ambiguities = Vec::new();
    for pair in entries.windows(2) {
        if pair[1].unix_nanos < pair[0].unix_nanos {
            clock_ambiguities.push(ClockAmbiguity {
                earlier: pair[0].citation.clone(),
                later: pair[1].citation.clone(),
                earlier_unix_nanos: pair[0].unix_nanos,
                later_unix_nanos: pair[1].unix_nanos,
            });
        }
    }
    limits.check(
        DebuggerLimit::Diagnostics,
        clock_ambiguities.len(),
        DebuggerOperation::BuildTimeline,
    )?;
    Ok(Timeline { subject_id, entries, clock_ambiguities })
}

const fn boundary(kind: ObservationKind) -> BoundaryKind {
    match kind {
        ObservationKind::SpanStarted(kind) => BoundaryKind::Started(kind),
        ObservationKind::Diagnostic(code) => BoundaryKind::Diagnostic(code),
        ObservationKind::SpanEnded(outcome) => BoundaryKind::Ended(outcome),
    }
}

fn resource_observations(evidence: &SelectedEvidence) -> Vec<ResourceObservation> {
    evidence
        .attributes()
        .iter()
        .filter(|attribute| {
            matches!(
                attribute.key(),
                SafeAttributeKey::BudgetUnits
                    | SafeAttributeKey::CpuNanos
                    | SafeAttributeKey::MemoryBytes
                    | SafeAttributeKey::InputTokens
                    | SafeAttributeKey::OutputTokens
                    | SafeAttributeKey::CostMicrounits
                    | SafeAttributeKey::QueueDepth
                    | SafeAttributeKey::DroppedCount
            )
        })
        .map(|attribute| ResourceObservation { key: attribute.key(), value: attribute.value() })
        .collect()
}

fn normalize_outcome(
    evidence: &SelectedEvidence,
    span_kind: Option<SpanKind>,
) -> Option<OutcomeClass> {
    match evidence.kind() {
        ObservationKind::SpanStarted(_) => None,
        ObservationKind::SpanEnded(SpanOutcome::Ok) => {
            matches!(span_kind, Some(SpanKind::AgentTurn))
                .then_some(OutcomeClass::Task(TaskOutcome::Success))
        }
        ObservationKind::SpanEnded(SpanOutcome::Cancelled) => {
            Some(OutcomeClass::Task(TaskOutcome::CancelledByTaskPolicy))
        }
        ObservationKind::SpanEnded(SpanOutcome::Indeterminate) => {
            Some(OutcomeClass::Task(TaskOutcome::Indeterminate))
        }
        ObservationKind::SpanEnded(outcome) => Some(outcome_for_span(evidence, span_kind, outcome)),
        ObservationKind::Diagnostic(code) => diagnostic_outcome(code),
    }
}

const fn outcome_for_span(
    evidence: &SelectedEvidence,
    started_kind: Option<SpanKind>,
    _outcome: SpanOutcome,
) -> OutcomeClass {
    if evidence.binding().gate_id().is_some() {
        OutcomeClass::Task(TaskOutcome::RequirementFailure)
    } else if evidence.binding().provider_profile_id().is_some() {
        OutcomeClass::Infrastructure(InfrastructureOutcome::ProviderFailure)
    } else if evidence.binding().tool_descriptor_digest().is_some() {
        OutcomeClass::Infrastructure(InfrastructureOutcome::ToolFailure)
    } else if matches!(started_kind, Some(SpanKind::Recovery)) {
        OutcomeClass::Infrastructure(InfrastructureOutcome::StorageFailure)
    } else {
        OutcomeClass::Task(TaskOutcome::RequirementFailure)
    }
}

const fn diagnostic_outcome(code: DiagnosticCode) -> Option<OutcomeClass> {
    match code {
        DiagnosticCode::ProviderRequestFailed => {
            Some(OutcomeClass::Infrastructure(InfrastructureOutcome::ProviderFailure))
        }
        DiagnosticCode::ToolDispatchFailed => {
            Some(OutcomeClass::Infrastructure(InfrastructureOutcome::ToolFailure))
        }
        DiagnosticCode::GateFailed => Some(OutcomeClass::Task(TaskOutcome::RequirementFailure)),
        DiagnosticCode::GateBlocked => Some(OutcomeClass::Task(TaskOutcome::Blocked)),
        DiagnosticCode::BudgetExhausted => {
            Some(OutcomeClass::Infrastructure(InfrastructureOutcome::SandboxFailure))
        }
        DiagnosticCode::CancellationRequested | DiagnosticCode::CancellationObserved => {
            Some(OutcomeClass::Task(TaskOutcome::CancelledByTaskPolicy))
        }
        DiagnosticCode::RecoveryFailed
        | DiagnosticCode::ExporterFailed
        | DiagnosticCode::BufferDropped => {
            Some(OutcomeClass::Infrastructure(InfrastructureOutcome::StorageFailure))
        }
        _ => None,
    }
}

fn timeline_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Report,
        DebuggerOperation::BuildTimeline,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
