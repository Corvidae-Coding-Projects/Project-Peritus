//! Active owned-process control, observation, finalization, and recovery.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_policy::AuthorityInstant;
use peritus_process::{
    CancellationReason as ProcessCancellation, OwnedProcess, ProcessControl, ProcessCursor,
    OutputCompleteness, OutputStream, ProcessSignal, ProcessStore, TerminalResult, TerminalSize,
};
use peritus_tool_protocol::{CancellationReason, PreparedToolCall, ToolControl, ToolResult};
use peritus_tool_router::{
    ControlRetryability, DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution,
};
use peritus_types::{EventId, ProcessId, Sha256Digest};

use super::{failure, progress, terminal};

const EVENT_PAGE: usize = 256;

/// Owned C2 execution projected through the C4 active-execution protocol.
pub struct ShellExecution {
    prepared: PreparedToolCall,
    owner: Option<OwnedProcess>,
    control: ProcessControl,
    process_store: ProcessStore,
    process_id: ProcessId,
    artifacts: ArtifactStore,
    creating_event: EventId,
    cursor: ProcessCursor,
    next_progress: u64,
    started_at: Option<AuthorityInstant>,
    last_observed_at: Option<AuthorityInstant>,
    progress_truncated: bool,
    completed: Option<CompletedProcessEvidence>,
    terminal: Option<ToolResult>,
    pending_progress: Option<PendingProgress>,
}

/// Already terminal C2 evidence projected once through the ordinary C4 settlement path.
pub struct RecoveredTerminalExecution {
    prepared: PreparedToolCall,
    terminal: ToolResult,
    frontier: u64,
}

#[derive(Clone)]
struct PendingProgress {
    update: ExecutionUpdate,
    cursor: ProcessCursor,
    next_progress: u64,
    progress_truncated: bool,
}

struct CompletedProcessEvidence {
    terminal: TerminalResult,
    retained_output: Vec<u8>,
}

impl ShellExecution {
    pub(crate) fn new(
        prepared: PreparedToolCall,
        owner: OwnedProcess,
        process_store: ProcessStore,
        process_id: ProcessId,
        artifacts: ArtifactStore,
        creating_event: EventId,
        started_at: AuthorityInstant,
    ) -> Self {
        let control = owner.control();
        Self {
            prepared,
            owner: Some(owner),
            control,
            process_store,
            process_id,
            artifacts,
            creating_event,
            cursor: ProcessCursor::after(0),
            next_progress: 0,
            started_at: Some(started_at),
            last_observed_at: Some(started_at),
            progress_truncated: false,
            completed: None,
            terminal: None,
            pending_progress: None,
        }
    }

    /// Reattaches the exact independently retained process owner at its durable C4 frontier.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn reattached(
        prepared: PreparedToolCall,
        owner: OwnedProcess,
        process_store: ProcessStore,
        process_id: ProcessId,
        artifacts: ArtifactStore,
        creating_event: EventId,
        next_progress: u64,
        process_cursor: u64,
        observed_at: AuthorityInstant,
        progress_truncated: bool,
    ) -> Self {
        let control = owner.control();
        Self {
            prepared,
            owner: Some(owner),
            control,
            process_store,
            process_id,
            artifacts,
            creating_event,
            cursor: ProcessCursor::after(process_cursor),
            next_progress,
            started_at: Some(AuthorityInstant::new(
                peritus_types::Generation::first(),
                20,
            )),
            last_observed_at: Some(observed_at),
            progress_truncated,
            completed: None,
            terminal: None,
            pending_progress: None,
        }
    }

    fn observe_time(&mut self, observed_at: AuthorityInstant) -> Result<(), DispatchFailure> {
        if self.last_observed_at.is_some_and(|prior| {
            prior.epoch() != observed_at.epoch() || prior.tick_millis() > observed_at.tick_millis()
        }) {
            return Err(failure::adapter(
                "shell-observation-time",
                "authority observation time regressed or crossed epochs",
            ));
        }
        self.started_at.get_or_insert(observed_at);
        self.last_observed_at = Some(observed_at);
        Ok(())
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)?;
        if let Some(pending) = &self.pending_progress {
            return Ok(pending.update.clone());
        }
        if let Some(result) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|error| failure::adapter("shell-terminal-repeat", &error.to_string()));
        }
        let mut cursor = self.cursor;
        let mut next_progress = self.next_progress;
        let mut progress_truncated = self.progress_truncated;
        let page_capacity = usize::try_from(self.prepared.call().limits().progress_events())
            .unwrap_or(usize::MAX)
            .min(EVENT_PAGE);
        let mut updates = Vec::with_capacity(page_capacity);
        if next_progress == 0 && updates.len() < page_capacity {
            updates.push(
                progress::started(&self.prepared, 0, observed_at)
                    .map_err(|error| failure::adapter("shell-progress", &error.to_string()))?,
            );
            next_progress = 1;
        }
        let events = self.control.read_events(cursor, page_capacity.saturating_sub(updates.len()));
        for event in &events {
            if event.loss().is_some()
                || event.sequence() > cursor.sequence().saturating_add(1)
            {
                progress_truncated = true;
            }
            cursor = ProcessCursor::after_event(event);
            updates.push(
                progress::event(&self.prepared, next_progress, event, observed_at)
                    .map_err(|error| failure::adapter("shell-progress", &error.to_string()))?,
            );
            next_progress = next_progress.checked_add(1).ok_or_else(|| {
                failure::adapter("shell-progress-frontier", "progress frontier overflowed")
            })?;
        }
        let has_more = !self.control.read_events(cursor, 1).is_empty();
        let result = if !has_more
            && (self.completed.is_some()
                || self.control.terminal_result().is_some()
                || self.control.owner_finished())
        {
            match self.finalize(observed_at, next_progress, progress_truncated) {
                Ok(result) => Some(result),
                Err(dispatch_failure) => {
                    let dispatch_failure = if dispatch_failure.failure().retryability()
                        == peritus_tool_protocol::Retryability::AfterRecovery
                    {
                        dispatch_failure
                    } else {
                        failure::settlement_projection(&dispatch_failure)
                    };
                    let update = ExecutionUpdate::settlement_pending(
                        &self.prepared,
                        updates,
                        dispatch_failure,
                    )
                    .map_err(|error| {
                        failure::adapter("shell-settlement-envelope", &error.to_string())
                    })?;
                    self.pending_progress = Some(PendingProgress {
                        update: update.clone(),
                        cursor,
                        next_progress,
                        progress_truncated,
                    });
                    return Ok(update);
                }
            }
        } else {
            None
        };
        let update = ExecutionUpdate::new(&self.prepared, updates, result)
            .map_err(|error| failure::adapter("shell-progress-envelope", &error.to_string()))?;
        self.pending_progress = Some(PendingProgress {
            update: update.clone(),
            cursor,
            next_progress,
            progress_truncated,
        });
        Ok(update)
    }

    fn finalize(
        &mut self,
        observed_at: AuthorityInstant,
        progress_frontier: u64,
        progress_truncated: bool,
    ) -> Result<ToolResult, DispatchFailure> {
        self.capture_terminal()?;
        self.complete_artifact_publication()?;
        let completed = self.completed.as_ref().ok_or_else(|| {
            failure::settlement_invariant(
                "shell-settlement-missing",
                "completed process evidence disappeared before terminal projection",
            )
        })?;
        let result = terminal::build(
            &self.prepared,
            &completed.terminal,
            &completed.retained_output,
            self.started_at.unwrap_or(observed_at),
            observed_at,
            progress_frontier,
            progress_truncated,
        )
        .map_err(|error| failure::settlement_projection(&error))?;
        self.terminal = Some(result.clone());
        Ok(result)
    }

    fn capture_terminal(&mut self) -> Result<(), DispatchFailure> {
        if self.completed.is_some() {
            return Ok(());
        }
        if let Some(owner) = self.owner.take() {
            return match owner.wait_and_publish(&self.artifacts, self.creating_event) {
                Ok(terminal) => self.retain_completed(terminal),
                Err(error) => {
                    let failure = failure::process(error.process_error());
                    let terminal = error
                        .terminal_result()
                        .cloned()
                        .or_else(|| self.control.terminal_result())
                        .or_else(|| self.process_store.terminal_result(self.process_id).ok());
                    if let Some(terminal) = terminal {
                        self.retain_completed(terminal)?;
                        if self
                            .completed
                            .as_ref()
                            .is_some_and(|evidence| evidence.terminal.artifact_publication_complete())
                        {
                            return Ok(());
                        }
                    }
                    Err(failure)
                }
            };
        }
        let terminal = self
            .control
            .terminal_result()
            .map(Ok)
            .unwrap_or_else(|| self.process_store.terminal_result(self.process_id))
            .map_err(|error| failure::process(&error))?;
        self.retain_completed(terminal)
    }

    fn complete_artifact_publication(&mut self) -> Result<(), DispatchFailure> {
        let Some(completed) = &self.completed else {
            return Err(failure::settlement_invariant(
                "shell-settlement-missing",
                "artifact publication has no retained terminal result",
            ));
        };
        if completed.terminal.artifact_publication_complete() {
            return Ok(());
        }
        match self.process_store.retry_artifact_publication(
            self.process_id,
            &self.artifacts,
            self.creating_event,
        ) {
            Ok(terminal) => {
                self.retain_completed(terminal)?;
                if self
                    .completed
                    .as_ref()
                    .is_some_and(|evidence| evidence.terminal.artifact_publication_complete())
                {
                    Ok(())
                } else {
                    Err(failure::settlement_invariant(
                        "shell-artifact-settlement",
                        "artifact publication returned without a complete durable terminal result",
                    ))
                }
            }
            Err(error) => {
                let failure = failure::process(error.process_error());
                let terminal = error
                    .terminal_result()
                    .cloned()
                    .or_else(|| self.control.terminal_result())
                    .or_else(|| self.process_store.terminal_result(self.process_id).ok());
                if let Some(terminal) = terminal {
                    self.retain_completed(terminal)?;
                    if self
                        .completed
                        .as_ref()
                        .is_some_and(|evidence| evidence.terminal.artifact_publication_complete())
                    {
                        return Ok(());
                    }
                }
                Err(failure)
            }
        }
    }

    fn retain_completed(&mut self, terminal: TerminalResult) -> Result<(), DispatchFailure> {
        if terminal.process_id() != self.process_id {
            return Err(failure::settlement_invariant(
                "shell-settlement-identity",
                "retained terminal result belongs to another process",
            ));
        }
        self.completed = Some(CompletedProcessEvidence {
            terminal,
            retained_output: self.control.retained_output(),
        });
        Ok(())
    }

    fn request_cancellation(&self, reason: CancellationReason) -> Result<(), DispatchFailure> {
        if self.control.terminal_result().is_some() || self.control.owner_finished() {
            return Ok(());
        }
        match self.control.cancel(cancellation(reason)) {
            Ok(()) => Ok(()),
            Err(_)
                if self.control.terminal_result().is_some() || self.control.owner_finished() =>
            {
                Ok(())
            }
            Err(error) => Err(failure::control(&error)),
        }
    }

    fn apply_control(&self, control: ToolControl) -> Result<(), DispatchFailure> {
        match control {
            ToolControl::Poll => Ok(()),
            ToolControl::Stdin(bytes) => {
                self.control.write_stdin(bytes).map_err(|error| failure::control(&error))
            }
            ToolControl::Resize { rows, columns } => {
                let size = TerminalSize::new(rows, columns, 0, 0)
                    .map_err(|error| failure::control(&error))?;
                self.control.resize(size).map_err(|error| failure::control(&error))
            }
            ToolControl::Signal(name) => {
                let signal = match name.as_str() {
                    "INT" | "SIGINT" | "interrupt" => ProcessSignal::Interrupt,
                    "TERM" | "SIGTERM" | "terminate" => ProcessSignal::Terminate,
                    _ => {
                        return Err(failure::invalid_control(
                            "shell-unsupported-signal",
                            "signal must be INT, SIGINT, TERM, SIGTERM, interrupt, or terminate",
                        ));
                    }
                };
                self.control.signal(signal).map_err(|error| failure::control(&error))
            }
            ToolControl::Cancel(reason) => self.request_cancellation(reason),
        }
    }
}

impl RecoveredTerminalExecution {
    /// Rebuilds terminal C4 evidence from an exact retained owner and its final stream snapshots.
    ///
    /// # Errors
    /// Returns a settlement failure when the retained owner, durable terminal result, artifact
    /// publication, or reconstructed C4 envelope cannot be proven exact.
    #[allow(clippy::too_many_arguments)]
    pub fn from_retained(
        prepared: PreparedToolCall,
        owner: OwnedProcess,
        process_store: ProcessStore,
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        artifacts: ArtifactStore,
        creating_event: EventId,
        finished_at: AuthorityInstant,
        frontier: u64,
        progress_truncated: bool,
    ) -> Result<Self, DispatchFailure> {
        let control = owner.control();
        let mut terminal = match owner.wait_and_publish(&artifacts, creating_event) {
            Ok(terminal) => terminal,
            Err(error) => error
                .terminal_result()
                .cloned()
                .or_else(|| control.terminal_result())
                .or_else(|| process_store.terminal_result(process_id).ok())
                .ok_or_else(|| failure::process(error.process_error()))?,
        };
        if !terminal.artifact_publication_complete() {
            terminal = process_store
                .retry_artifact_publication(process_id, &artifacts, creating_event)
                .map_err(|error| failure::process(error.process_error()))?;
        }
        validate_recovered_terminal(&terminal, process_id, plan_digest)?;
        let retained = control.retained_output();
        let terminal = terminal::build(
            &prepared,
            &terminal,
            &retained,
            AuthorityInstant::new(peritus_types::Generation::first(), 20),
            finished_at,
            frontier,
            progress_truncated,
        )?;
        Ok(Self { prepared, terminal, frontier })
    }

    /// Rebuilds terminal C4 evidence from the exact durable C2 result and published artifacts.
    ///
    /// # Errors
    /// Returns a settlement failure when the C2 terminal binding, artifact publication, retained
    /// output selection, or reconstructed C4 envelope cannot be proven exact.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        prepared: PreparedToolCall,
        process_store: ProcessStore,
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        artifacts: ArtifactStore,
        creating_event: EventId,
        retained_window_bytes: u64,
        finished_at: AuthorityInstant,
        frontier: u64,
        progress_truncated: bool,
    ) -> Result<Self, DispatchFailure> {
        let mut terminal = process_store
            .terminal_result(process_id)
            .map_err(|error| failure::process(&error))?;
        if !terminal.artifact_publication_complete() {
            terminal = process_store
                .retry_artifact_publication(process_id, &artifacts, creating_event)
                .map_err(|error| failure::process(error.process_error()))?;
        }
        validate_recovered_terminal(&terminal, process_id, plan_digest)?;
        let retained = recovered_retained_output(&artifacts, &terminal, retained_window_bytes)?;
        let terminal = terminal::build(
            &prepared,
            &terminal,
            &retained,
            AuthorityInstant::new(peritus_types::Generation::first(), 20),
            finished_at,
            frontier,
            progress_truncated,
        )?;
        Ok(Self { prepared, terminal, frontier })
    }

    /// Borrows the reconstructed exact terminal envelope.
    #[must_use]
    pub const fn result(&self) -> &ToolResult {
        &self.terminal
    }

    fn update(&self) -> Result<ExecutionUpdate, DispatchFailure> {
        ExecutionUpdate::new(&self.prepared, Vec::new(), Some(self.terminal.clone()))
            .map_err(|error| failure::adapter("shell-recovered-terminal", &error.to_string()))
    }
}

fn validate_recovered_terminal(
    terminal: &TerminalResult,
    process_id: ProcessId,
    plan_digest: Sha256Digest,
) -> Result<(), DispatchFailure> {
    if terminal.process_id() != process_id || terminal.plan_digest() != plan_digest {
        return Err(failure::settlement_invariant(
            "shell-recovered-terminal-identity",
            "recovered terminal result belongs to another process or execution plan",
        ));
    }
    Ok(())
}

impl ToolExecution for RecoveredTerminalExecution {
    fn poll(&mut self, _observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.update()
    }

    fn control(
        &mut self,
        _control: ToolControl,
        _observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.update()
    }

    fn cancel(
        &mut self,
        _reason: CancellationReason,
        _observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.update()
    }

    fn recover(
        &mut self,
        _observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        self.update().map(RecoveryObservation::Completed)
    }

    fn acknowledge_progress(&mut self, next_frontier: u64) -> Result<(), DispatchFailure> {
        if next_frontier == self.frontier {
            Ok(())
        } else {
            Err(failure::adapter(
                "shell-recovered-progress-ack",
                "router changed the recovered terminal progress frontier",
            ))
        }
    }
}

fn recovered_retained_output(
    artifacts: &ArtifactStore,
    terminal: &TerminalResult,
    retained_window_bytes: u64,
) -> Result<Vec<u8>, DispatchFailure> {
    let capacity = usize::try_from(retained_window_bytes).map_err(|_| {
        failure::settlement_invariant(
            "shell-recovered-output-window",
            "retained output window exceeds this host",
        )
    })?;
    if capacity == 0 {
        return Ok(Vec::new());
    }
    if !terminal.output().is_complete() {
        return Err(failure::settlement_invariant(
            "shell-recovered-output-incomplete",
            "non-retained terminal output is not complete enough to reconstruct its live window",
        ));
    }
    let total = terminal.output().streams().iter().try_fold(0_u64, |total, stream| {
        total.checked_add(stream.observed()).ok_or_else(|| {
            failure::settlement_invariant(
                "shell-recovered-output-size",
                "terminal output accounting overflowed during recovery",
            )
        })
    })?;
    let populated = terminal
        .output()
        .streams()
        .iter()
        .filter(|stream| stream.observed() != 0)
        .count();
    if total > retained_window_bytes && populated > 1 {
        return Err(failure::settlement_invariant(
            "shell-recovered-output-order",
            "multiple streams exceed the retained window without an exact durable stream selection",
        ));
    }

    let mut output = Vec::with_capacity(capacity.saturating_add(2));
    for stream in [OutputStream::Stdout, OutputStream::Stderr, OutputStream::Terminal] {
        let matching = terminal
            .output()
            .streams()
            .iter()
            .filter(|accounting| accounting.stream() == stream)
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            return Err(failure::settlement_invariant(
                "shell-recovered-output-accounting",
                "terminal output contains duplicate stream accounting",
            ));
        }
        let Some(accounting) = matching.first().copied() else { continue };
        if accounting.observed() == 0 {
            continue;
        }
        let matching = terminal
            .artifacts()
            .iter()
            .filter(|artifact| artifact.stream() == stream)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(failure::settlement_invariant(
                "shell-recovered-output-artifact",
                "complete terminal stream does not have one exact durable artifact",
            ));
        }
        let artifact = matching[0];
        if artifact.completeness() != OutputCompleteness::Complete
            || artifact.size() != accounting.observed()
            || artifact.start_offset() != 0
            || artifact.end_offset() != artifact.size()
        {
            return Err(failure::settlement_invariant(
                "shell-recovered-output-artifact",
                "terminal artifact differs from its complete stream accounting",
            ));
        }
        let length_u64 = if total <= retained_window_bytes {
            accounting.observed()
        } else {
            accounting.observed().min(retained_window_bytes)
        };
        let length = usize::try_from(length_u64).map_err(|_| {
            failure::settlement_invariant(
                "shell-recovered-output-size",
                "retained output artifact tail exceeds this host",
            )
        })?;
        let offset = artifact.size().checked_sub(length_u64).ok_or_else(|| {
            failure::settlement_invariant(
                "shell-recovered-output-size",
                "retained output artifact tail exceeds the complete stream",
            )
        })?;
        let mut reader = artifacts
            .open_read(ArtifactDigest::from_sha256(artifact.digest()))
            .map_err(|error| {
                failure::adapter("shell-recovered-output-open", &error.to_string())
            })?;
        let chunk = reader
            .read_chunk_at(offset, length)
            .map_err(|error| {
                failure::adapter("shell-recovered-output-read", &error.to_string())
            })?
            .ok_or_else(|| {
                failure::settlement_invariant(
                    "shell-recovered-output-read",
                    "nonempty terminal artifact tail returned no bytes",
                )
            })?;
        if chunk.bytes().len() != length {
            return Err(failure::settlement_invariant(
                "shell-recovered-output-read",
                "terminal artifact tail was shorter than its authenticated range",
            ));
        }
        if !output.is_empty() && !output.ends_with(b"\n") {
            output.push(b'\n');
        }
        output.extend_from_slice(chunk.bytes());
    }
    Ok(output)
}

impl ToolExecution for ShellExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll_owned(observed_at)
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)
            .map_err(|error| error.rejecting_control(ControlRetryability::CorrectRequest))?;
        self.apply_control(control)?;
        self.poll_owned(observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)
            .map_err(|error| error.rejecting_control(ControlRetryability::CorrectRequest))?;
        if self.control.terminal_result().is_none() && !self.control.owner_finished() {
            self.request_cancellation(reason)?;
        }
        self.poll_owned(observed_at)
    }

    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        let update = self.poll_owned(observed_at)?;
        if update.terminal().is_some() {
            Ok(RecoveryObservation::Completed(update))
        } else {
            Ok(RecoveryObservation::Active(update))
        }
    }

    fn acknowledge_progress(
        &mut self,
        next_frontier: u64,
    ) -> Result<(), DispatchFailure> {
        let pending = self.pending_progress.as_ref().ok_or_else(|| {
            failure::adapter(
                "shell-progress-ack",
                "router acknowledged progress without a retained pending page",
            )
        })?;
        if pending.next_progress != next_frontier {
            return Err(failure::adapter(
                "shell-progress-ack",
                "router progress acknowledgement differs from the retained page frontier",
            ));
        }
        let pending = self.pending_progress.take().expect("checked pending progress");
        self.cursor = pending.cursor;
        self.next_progress = pending.next_progress;
        self.progress_truncated = pending.progress_truncated;
        Ok(())
    }
}

const fn cancellation(reason: CancellationReason) -> ProcessCancellation {
    match reason {
        CancellationReason::Requested => ProcessCancellation::User,
        CancellationReason::Deadline => ProcessCancellation::Deadline,
        CancellationReason::Shutdown => ProcessCancellation::SupervisorShutdown,
        CancellationReason::Recovery => ProcessCancellation::BackendFailure,
    }
}
