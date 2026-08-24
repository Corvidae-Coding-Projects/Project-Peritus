//! Deterministic terminal classification and resource observations.

use std::{sync::Arc, time::Instant};

use crate::{
    CancellationReason, EscalationRecord, ExecutionPlan, OsExitObservation, OutputSummary,
    ProcessError, ProcessInstant, ProcessStore, StopTrigger, TerminalDisposition, TerminalRecovery,
    TerminalResult, control::SharedObservation, platform::PlatformExit,
};

use super::{elapsed_millis, publish_terminal};

pub(crate) fn record_preparation_failure(
    store: &ProcessStore,
    plan: &ExecutionPlan,
    backend_cleanup_complete: bool,
) -> Result<(), ProcessError> {
    let process_id = plan.identity().process_id();
    let trigger = StopTrigger::new(1, CancellationReason::BackendFailure);
    store.record_phase(process_id, crate::LifecyclePhase::Starting)?;
    store.record_stopping(process_id, trigger)?;
    store.record_failed_closed(
        process_id,
        OsExitObservation::Unavailable,
        0,
        0,
        0,
        true,
        backend_cleanup_complete,
    )?;
    let result = TerminalResult::new(
        process_id,
        plan.digest(),
        TerminalDisposition::SandboxDenied,
        OsExitObservation::Unavailable,
        Some(trigger),
        EscalationRecord::new(false, false, true),
        None,
        ProcessInstant::from_millis(0),
        OutputSummary::new(Vec::new(), 0),
        Vec::new(),
        true,
        backend_cleanup_complete,
        TerminalRecovery::OriginalOwner,
    );
    store.record_terminal(process_id, &result)
}

#[cfg(unix)]
pub(super) fn convert_exit(exit: &PlatformExit) -> OsExitObservation {
    match exit {
        PlatformExit::Code(code) => OsExitObservation::Code(*code),
        PlatformExit::Signal(signal) => OsExitObservation::Signal(*signal),
        PlatformExit::SignalName(signal) => OsExitObservation::SignalName(signal.clone()),
        PlatformExit::PlatformException(code) => OsExitObservation::PlatformException(*code),
    }
}

#[cfg(not(unix))]
pub(super) const fn convert_exit(exit: &PlatformExit) -> OsExitObservation {
    match exit {
        PlatformExit::Code(code) => OsExitObservation::Code(*code),
        PlatformExit::PlatformException(code) => OsExitObservation::PlatformException(*code),
    }
}

pub(super) fn publish_spawn_failure(
    store: &ProcessStore,
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
    began: Instant,
    backend_cleanup_complete: bool,
    _error: ProcessError,
) -> Result<TerminalResult, ProcessError> {
    let process_id = plan.identity().process_id();
    store.record_spawn_failed(process_id, backend_cleanup_complete)?;
    let result = TerminalResult::new(
        process_id,
        plan.digest(),
        TerminalDisposition::SpawnFailed,
        OsExitObservation::Unavailable,
        None,
        EscalationRecord::new(false, false, true),
        None,
        ProcessInstant::from_millis(elapsed_millis(began)),
        OutputSummary::new(Vec::new(), 0),
        Vec::new(),
        true,
        backend_cleanup_complete,
        TerminalRecovery::OriginalOwner,
    );
    store.record_terminal(process_id, &result)?;
    publish_terminal(shared, plan, &result);
    Ok(result)
}

#[derive(Clone, Copy)]
pub(super) enum CompletionState {
    Complete,
    Incomplete,
}

impl CompletionState {
    pub(super) const fn from_complete(complete: bool) -> Self {
        if complete { Self::Complete } else { Self::Incomplete }
    }

    const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[derive(Clone, Copy)]
pub(super) enum FailureState {
    Healthy,
    Failed,
}

impl FailureState {
    pub(super) const fn from_failed(failed: bool) -> Self {
        if failed { Self::Failed } else { Self::Healthy }
    }

    const fn failed(self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[derive(Clone, Copy)]
pub(super) struct CompletionFacts {
    reader: FailureState,
    tree: CompletionState,
    tasks: CompletionState,
    owner: FailureState,
}

impl CompletionFacts {
    pub(super) const fn new(
        reader: FailureState,
        tree: CompletionState,
        tasks: CompletionState,
        owner: FailureState,
    ) -> Self {
        Self { reader, tree, tasks, owner }
    }
}

pub(super) const fn classify(
    trigger: Option<StopTrigger>,
    exit: &OsExitObservation,
    facts: CompletionFacts,
    resource_exceeded: bool,
) -> TerminalDisposition {
    if facts.owner.failed()
        || !facts.tree.is_complete()
        || !facts.tasks.is_complete()
        || facts.reader.failed()
    {
        return TerminalDisposition::SupervisorFailed;
    }
    if let Some(trigger) = trigger {
        return match trigger.reason() {
            CancellationReason::User
            | CancellationReason::SupervisorShutdown
            | CancellationReason::LeaseFence => TerminalDisposition::Cancelled,
            CancellationReason::Deadline => TerminalDisposition::TimedOut,
            CancellationReason::OutputLimit => TerminalDisposition::OutputLimit,
            CancellationReason::ResourceLimit => TerminalDisposition::ResourceLimit,
            CancellationReason::BackendFailure => TerminalDisposition::SandboxDenied,
        };
    }
    if resource_exceeded {
        return TerminalDisposition::ResourceLimit;
    }
    match exit {
        OsExitObservation::Code(_) => TerminalDisposition::Exited,
        OsExitObservation::Signal(_)
        | OsExitObservation::SignalName(_)
        | OsExitObservation::PlatformException(_) => TerminalDisposition::Signalled,
        OsExitObservation::Unavailable => TerminalDisposition::SupervisorFailed,
    }
}
