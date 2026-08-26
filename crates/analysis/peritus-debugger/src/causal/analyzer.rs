//! Fixed deterministic analyzer registry.

use std::collections::BTreeMap;

use crate::{
    AlternativeCauses, AmbiguityFlag, CauseDerivation, ConfidenceBasis, ConfidenceMillionths,
    DebuggerError, DebuggerLimit, DebuggerLimits, DebuggerOperation, DiagnosticText,
    EvidenceCitation, FailureCategory, InfrastructureOutcome, OutcomeClass, RootCauseCandidate,
    SubjectId, TaskOutcome, Timeline, TimelineEntry, TraceSelectionManifest,
};
use peritus_trace::{DiagnosticCode, SpanKind};

/// Stable analyzer registry tag and pattern signature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum AnalyzerSignature {
    /// Terminal outcome normalization.
    TerminalOutcome = 1,
    /// Span did not close.
    IncompleteSpan = 2,
    /// Diagnostic recurred within one attempt.
    RepeatedDiagnostic = 3,
    /// Provider failure mapping.
    ProviderFailure = 4,
    /// Tool failure mapping.
    ToolFailure = 5,
    /// Gate task/infrastructure mapping.
    GateFailure = 6,
    /// Storage and recovery mapping.
    StorageFailure = 7,
    /// Selected-only evidence has a causal gap.
    CausalGap = 8,
    /// Retry loop signature.
    RetryLoop = 9,
    /// Cancellation signature.
    Cancellation = 10,
    /// Resource pressure signature.
    ResourcePressure = 11,
    /// Successful path signature.
    SuccessPath = 12,
}

/// One typed deterministic finding used for clustering and report construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisFinding {
    subject_id: SubjectId,
    signature: AnalyzerSignature,
    outcome: OutcomeClass,
    category: Option<FailureCategory>,
    citations: Vec<EvidenceCitation>,
    cause_id: Option<crate::CauseId>,
}

impl AnalysisFinding {
    /// Returns the source subject.
    #[must_use]
    pub const fn subject_id(&self) -> SubjectId {
        self.subject_id
    }
    /// Returns the stable analyzer signature.
    #[must_use]
    pub const fn signature(&self) -> AnalyzerSignature {
        self.signature
    }
    /// Returns normalized task/infrastructure outcome.
    #[must_use]
    pub const fn outcome(&self) -> OutcomeClass {
        self.outcome
    }
    /// Returns a closed failure category, absent for successful signatures.
    #[must_use]
    pub const fn category(&self) -> Option<FailureCategory> {
        self.category
    }
    /// Borrows exact source citations.
    #[must_use]
    pub fn citations(&self) -> &[EvidenceCitation] {
        &self.citations
    }
    /// Returns the associated candidate-cause identity, when failure-oriented.
    #[must_use]
    pub const fn cause_id(&self) -> Option<crate::CauseId> {
        self.cause_id
    }
}

/// Complete bounded deterministic analysis result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAnalysis {
    findings: Vec<AnalysisFinding>,
    causes: Vec<RootCauseCandidate>,
}

impl DeterministicAnalysis {
    /// Borrows findings in `(subject, analyzer, category, citations)` order.
    #[must_use]
    pub fn findings(&self) -> &[AnalysisFinding] {
        &self.findings
    }
    /// Borrows candidate causes in stable identity order.
    #[must_use]
    pub fn causes(&self) -> &[RootCauseCandidate] {
        &self.causes
    }
}

/// Runs every fixed schema-v1 deterministic analyzer.
///
/// # Errors
///
/// Rejects bound excess or any internally produced noncanonical cause/citation set.
pub fn analyze_timelines(
    manifest: &TraceSelectionManifest,
    timelines: &[Timeline],
    limits: DebuggerLimits,
) -> Result<DeterministicAnalysis, DebuggerError> {
    let mut findings = Vec::new();
    let mut causes = Vec::new();
    for timeline in timelines {
        analyze_terminal(manifest, timeline, &mut findings, &mut causes)?;
        analyze_incomplete(manifest, timeline, &mut findings, &mut causes)?;
        analyze_diagnostics(manifest, timeline, &mut findings, &mut causes)?;
        analyze_causal_gaps(manifest, timeline, &mut findings, &mut causes)?;
    }
    findings.sort_by(|left, right| {
        (left.subject_id, left.signature, left.category, &left.citations).cmp(&(
            right.subject_id,
            right.signature,
            right.category,
            &right.citations,
        ))
    });
    findings.dedup();
    causes.sort_by_key(RootCauseCandidate::id);
    causes.dedup_by_key(|cause| cause.id());
    limits.check(DebuggerLimit::Diagnostics, findings.len(), DebuggerOperation::AnalyzeCauses)?;
    limits.check(DebuggerLimit::Claims, causes.len(), DebuggerOperation::AnalyzeCauses)?;
    Ok(DeterministicAnalysis { findings, causes })
}

fn analyze_terminal(
    manifest: &TraceSelectionManifest,
    timeline: &Timeline,
    findings: &mut Vec<AnalysisFinding>,
    causes: &mut Vec<RootCauseCandidate>,
) -> Result<(), DebuggerError> {
    for entry in timeline.entries() {
        let Some(outcome) = entry.outcome() else { continue };
        if outcome.is_task_success() {
            findings.push(finding(timeline, AnalyzerSignature::SuccessPath, outcome, None, entry));
        } else if let Some(category) = super::rules::category_for_entry(entry) {
            push_cause(
                manifest,
                timeline,
                AnalyzerSignature::TerminalOutcome,
                outcome,
                category,
                "terminal evidence indicates a bounded task or infrastructure failure",
                vec![entry.citation().clone()],
                Vec::new(),
                AlternativeCauses::NoneKnown,
                super::rules::ambiguity_for_entry(entry, timeline),
                findings,
                causes,
            )?;
        }
    }
    Ok(())
}

fn analyze_incomplete(
    manifest: &TraceSelectionManifest,
    timeline: &Timeline,
    findings: &mut Vec<AnalysisFinding>,
    causes: &mut Vec<RootCauseCandidate>,
) -> Result<(), DebuggerError> {
    let mut open = BTreeMap::new();
    for entry in timeline.entries() {
        match entry.boundary() {
            crate::BoundaryKind::Started(kind) => {
                open.insert(entry.span_id(), (kind, entry));
            }
            crate::BoundaryKind::Ended(_) => {
                open.remove(&entry.span_id());
            }
            crate::BoundaryKind::Diagnostic(_) => {}
        }
    }
    for (_, (kind, entry)) in open {
        let (category, outcome) = match kind {
            SpanKind::Provider => (
                FailureCategory::ModelCompletion,
                OutcomeClass::Infrastructure(InfrastructureOutcome::ProviderFailure),
            ),
            SpanKind::Tool => (
                FailureCategory::ToolExecution,
                OutcomeClass::Infrastructure(InfrastructureOutcome::ToolFailure),
            ),
            SpanKind::Recovery => (
                FailureCategory::Recovery,
                OutcomeClass::Infrastructure(InfrastructureOutcome::StorageFailure),
            ),
            _ => (FailureCategory::ModelCompletion, OutcomeClass::Task(TaskOutcome::Indeterminate)),
        };
        push_cause(
            manifest,
            timeline,
            AnalyzerSignature::IncompleteSpan,
            outcome,
            category,
            "a selected span opened without a terminal observation",
            vec![entry.citation().clone()],
            Vec::new(),
            AlternativeCauses::NoneKnown,
            vec![AmbiguityFlag::IncompleteSpan],
            findings,
            causes,
        )?;
    }
    Ok(())
}

fn analyze_diagnostics(
    manifest: &TraceSelectionManifest,
    timeline: &Timeline,
    findings: &mut Vec<AnalysisFinding>,
    causes: &mut Vec<RootCauseCandidate>,
) -> Result<(), DebuggerError> {
    let mut by_code: BTreeMap<DiagnosticCode, Vec<&TimelineEntry>> = BTreeMap::new();
    for entry in timeline.entries() {
        if let crate::BoundaryKind::Diagnostic(code) = entry.boundary() {
            by_code.entry(code).or_default().push(entry);
        }
    }
    for (code, entries) in by_code {
        if code == DiagnosticCode::RetryScheduled && entries.len() < 2 {
            continue;
        }
        let Some((signature, outcome, category, statement)) = super::rules::diagnostic_rule(code)
        else {
            continue;
        };
        let support: Vec<_> = entries.iter().map(|entry| entry.citation().clone()).collect();
        let effective_signature = if entries.len() > 1 && code != DiagnosticCode::RetryScheduled {
            AnalyzerSignature::RepeatedDiagnostic
        } else {
            signature
        };
        let alternatives = super::rules::alternatives_for(category);
        push_cause(
            manifest,
            timeline,
            effective_signature,
            outcome,
            category,
            statement,
            support,
            super::rules::success_citations(timeline),
            alternatives,
            Vec::new(),
            findings,
            causes,
        )?;
    }
    Ok(())
}

fn analyze_causal_gaps(
    manifest: &TraceSelectionManifest,
    timeline: &Timeline,
    findings: &mut Vec<AnalysisFinding>,
    causes: &mut Vec<RootCauseCandidate>,
) -> Result<(), DebuggerError> {
    for entry in timeline.entries().iter().filter(|entry| !entry.missing_predecessors().is_empty())
    {
        push_cause(
            manifest,
            timeline,
            AnalyzerSignature::CausalGap,
            OutcomeClass::Task(TaskOutcome::Indeterminate),
            FailureCategory::ContextProvenance,
            "selected-only evidence omits one or more declared causal predecessors",
            vec![entry.citation().clone()],
            Vec::new(),
            AlternativeCauses::Categories(vec![FailureCategory::ContextSelection]),
            vec![AmbiguityFlag::MissingCausalPredecessor],
            findings,
            causes,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "analyzer output fields remain explicit")]
fn push_cause(
    manifest: &TraceSelectionManifest,
    timeline: &Timeline,
    signature: AnalyzerSignature,
    outcome: OutcomeClass,
    category: FailureCategory,
    statement: &'static str,
    mut support: Vec<EvidenceCitation>,
    mut contrary: Vec<EvidenceCitation>,
    alternatives: AlternativeCauses,
    mut ambiguities: Vec<AmbiguityFlag>,
    findings: &mut Vec<AnalysisFinding>,
    causes: &mut Vec<RootCauseCandidate>,
) -> Result<(), DebuggerError> {
    support.sort();
    support.dedup();
    contrary.sort();
    contrary.dedup();
    if !timeline.clock_ambiguities().is_empty() {
        ambiguities.push(AmbiguityFlag::ClockDisagreement);
    }
    ambiguities.sort();
    ambiguities.dedup();
    let basis = ConfidenceBasis::new(
        u32::try_from(support.len()).unwrap_or(u32::MAX),
        u32::try_from(contrary.len()).unwrap_or(u32::MAX),
        u32::try_from(ambiguities.len()).unwrap_or(u32::MAX),
        u32::try_from(support.len().saturating_sub(1)).unwrap_or(u32::MAX),
        u32::from(ambiguities.contains(&AmbiguityFlag::MissingCausalPredecessor)),
    );
    let cause = RootCauseCandidate::new(
        manifest,
        category,
        DiagnosticText::new(statement)?,
        support.clone(),
        contrary,
        alternatives,
        ConfidenceMillionths::calculate(basis)?,
        ambiguities,
        CauseDerivation::Deterministic,
    )?;
    findings.push(AnalysisFinding {
        subject_id: timeline.subject_id(),
        signature,
        outcome,
        category: Some(category),
        citations: support,
        cause_id: Some(cause.id()),
    });
    causes.push(cause);
    Ok(())
}

fn finding(
    timeline: &Timeline,
    signature: AnalyzerSignature,
    outcome: OutcomeClass,
    category: Option<FailureCategory>,
    entry: &TimelineEntry,
) -> AnalysisFinding {
    AnalysisFinding {
        subject_id: timeline.subject_id(),
        signature,
        outcome,
        category,
        citations: vec![entry.citation().clone()],
        cause_id: None,
    }
}
