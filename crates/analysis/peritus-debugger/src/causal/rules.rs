//! Frozen typed mappings used by the deterministic analyzer registry.

use std::collections::BTreeSet;

use crate::{
    AlternativeCauses, AmbiguityFlag, AnalyzerSignature, EvidenceCitation, FailureCategory,
    InfrastructureOutcome, OutcomeClass, TaskOutcome, Timeline, TimelineEntry,
};
use peritus_trace::DiagnosticCode;

pub(super) fn category_for_entry(entry: &TimelineEntry) -> Option<FailureCategory> {
    match entry.outcome()? {
        OutcomeClass::Task(TaskOutcome::Success) => None,
        OutcomeClass::Task(TaskOutcome::RequirementFailure) => {
            Some(FailureCategory::DeterministicGateFailure)
        }
        OutcomeClass::Task(TaskOutcome::Blocked) => Some(FailureCategory::ReviewUnresolvedBlocker),
        OutcomeClass::Task(TaskOutcome::CancelledByTaskPolicy) => {
            Some(FailureCategory::SchedulerCancellation)
        }
        OutcomeClass::Task(TaskOutcome::Indeterminate) => Some(FailureCategory::ModelCompletion),
        OutcomeClass::Infrastructure(InfrastructureOutcome::ProviderFailure) => {
            Some(FailureCategory::ProviderProtocol)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::ToolFailure) => {
            Some(FailureCategory::ToolExecution)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::WorkspaceFailure) => {
            Some(FailureCategory::Workspace)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::SandboxFailure) => {
            Some(FailureCategory::Resource)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::GateInfrastructureFailure) => {
            Some(FailureCategory::GateInfrastructureFailure)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::StorageFailure) => {
            Some(FailureCategory::Recovery)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::AuthorityFailure) => {
            Some(FailureCategory::AuthorityDenied)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::SchedulerFailure) => {
            Some(FailureCategory::SchedulerDependencyFailure)
        }
        OutcomeClass::Infrastructure(InfrastructureOutcome::IndeterminateInfrastructure) => {
            Some(FailureCategory::Projection)
        }
    }
}

pub(super) const fn diagnostic_rule(
    code: DiagnosticCode,
) -> Option<(AnalyzerSignature, OutcomeClass, FailureCategory, &'static str)> {
    use InfrastructureOutcome as I;
    match code {
        DiagnosticCode::ProviderRequestFailed => Some((
            AnalyzerSignature::ProviderFailure,
            OutcomeClass::Infrastructure(I::ProviderFailure),
            FailureCategory::ProviderProtocol,
            "provider request failed before a validated terminal result",
        )),
        DiagnosticCode::ToolDispatchFailed => Some((
            AnalyzerSignature::ToolFailure,
            OutcomeClass::Infrastructure(I::ToolFailure),
            FailureCategory::ToolExecution,
            "tool dispatch failed before a normalized successful result",
        )),
        DiagnosticCode::GateFailed => Some((
            AnalyzerSignature::GateFailure,
            OutcomeClass::Task(TaskOutcome::RequirementFailure),
            FailureCategory::DeterministicGateFailure,
            "deterministic gate evidence failed the task candidate",
        )),
        DiagnosticCode::GateBlocked => Some((
            AnalyzerSignature::GateFailure,
            OutcomeClass::Task(TaskOutcome::Blocked),
            FailureCategory::ReviewUnresolvedBlocker,
            "gate evidence records a blocked task path",
        )),
        DiagnosticCode::RetryScheduled => Some((
            AnalyzerSignature::RetryLoop,
            OutcomeClass::Infrastructure(I::IndeterminateInfrastructure),
            FailureCategory::SchedulerDependencyFailure,
            "bounded retry scheduling recurred in the attempt",
        )),
        DiagnosticCode::CancellationRequested | DiagnosticCode::CancellationObserved => Some((
            AnalyzerSignature::Cancellation,
            OutcomeClass::Task(TaskOutcome::CancelledByTaskPolicy),
            FailureCategory::SchedulerCancellation,
            "cancellation was requested or observed",
        )),
        DiagnosticCode::BudgetExhausted => Some((
            AnalyzerSignature::ResourcePressure,
            OutcomeClass::Infrastructure(I::SandboxFailure),
            FailureCategory::Resource,
            "a configured resource budget was exhausted",
        )),
        DiagnosticCode::RecoveryFailed
        | DiagnosticCode::ExporterFailed
        | DiagnosticCode::BufferDropped => Some((
            AnalyzerSignature::StorageFailure,
            OutcomeClass::Infrastructure(I::StorageFailure),
            FailureCategory::Recovery,
            "storage, export, buffer, or recovery evidence failed",
        )),
        _ => None,
    }
}

pub(super) fn alternatives_for(category: FailureCategory) -> AlternativeCauses {
    match category {
        FailureCategory::ProviderProtocol => {
            AlternativeCauses::Categories(vec![FailureCategory::ProviderTransport])
        }
        FailureCategory::ToolExecution => {
            AlternativeCauses::Categories(vec![FailureCategory::ToolResultNormalization])
        }
        FailureCategory::Recovery => AlternativeCauses::Categories(vec![
            FailureCategory::Journal,
            FailureCategory::Projection,
        ]),
        _ => AlternativeCauses::NoneKnown,
    }
}

pub(super) fn ambiguity_for_entry(
    entry: &TimelineEntry,
    timeline: &Timeline,
) -> Vec<AmbiguityFlag> {
    let mut flags = BTreeSet::new();
    if !entry.missing_predecessors().is_empty() {
        flags.insert(AmbiguityFlag::MissingCausalPredecessor);
    }
    if !timeline.clock_ambiguities().is_empty() {
        flags.insert(AmbiguityFlag::ClockDisagreement);
    }
    flags.into_iter().collect()
}

pub(super) fn success_citations(timeline: &Timeline) -> Vec<EvidenceCitation> {
    timeline
        .entries()
        .iter()
        .filter(|entry| entry.outcome().is_some_and(OutcomeClass::is_task_success))
        .map(|entry| entry.citation().clone())
        .collect()
}
