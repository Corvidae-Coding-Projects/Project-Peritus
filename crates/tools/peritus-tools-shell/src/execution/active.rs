//! Active owned-process control, observation, finalization, and recovery.

use peritus_artifact_store::ArtifactStore;
use peritus_policy::AuthorityInstant;
use peritus_process::{
    CancellationReason as ProcessCancellation, OwnedProcess, ProcessControl, ProcessCursor,
    ProcessSignal, TerminalSize,
};
use peritus_tool_protocol::{CancellationReason, PreparedToolCall, ToolControl, ToolResult};
use peritus_tool_router::{DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution};
use peritus_types::EventId;

use super::{failure, progress, terminal};

const EVENT_PAGE: usize = 256;

/// Owned C2 execution projected through the C4 active-execution protocol.
pub struct ShellExecution {
    prepared: PreparedToolCall,
    owner: Option<OwnedProcess>,
    control: ProcessControl,
    artifacts: ArtifactStore,
    creating_event: EventId,
    cursor: ProcessCursor,
    next_progress: u32,
    started_at: Option<AuthorityInstant>,
    last_observed_at: Option<AuthorityInstant>,
    progress_truncated: bool,
    terminal: Option<ToolResult>,
}

impl ShellExecution {
    pub(crate) fn new(
        prepared: PreparedToolCall,
        owner: OwnedProcess,
        artifacts: ArtifactStore,
        creating_event: EventId,
        started_at: AuthorityInstant,
    ) -> Self {
        let control = owner.control();
        Self {
            prepared,
            owner: Some(owner),
            control,
            artifacts,
            creating_event,
            cursor: ProcessCursor::after(0),
            next_progress: 0,
            started_at: Some(started_at),
            last_observed_at: Some(started_at),
            progress_truncated: false,
            terminal: None,
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
        if let Some(result) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|error| failure::adapter("shell-terminal-repeat", &error.to_string()));
        }
        let mut updates = Vec::new();
        if self.next_progress == 0 {
            self.push_progress(&mut updates, progress::started(&self.prepared, 0, observed_at))?;
        }
        let events = self.control.read_events(self.cursor, EVENT_PAGE);
        for event in &events {
            if event.sequence() > self.cursor.sequence().saturating_add(1) {
                self.progress_truncated = true;
            }
            self.cursor = ProcessCursor::after(event.sequence());
            let projected = progress::event(&self.prepared, self.next_progress, event, observed_at);
            self.push_progress(&mut updates, projected)?;
        }
        let result = if events.len() < EVENT_PAGE && self.control.terminal_result().is_some() {
            Some(self.finalize(observed_at)?)
        } else {
            None
        };
        ExecutionUpdate::new(&self.prepared, updates, result)
            .map_err(|error| failure::adapter("shell-progress-envelope", &error.to_string()))
    }

    fn push_progress(
        &mut self,
        target: &mut Vec<peritus_tool_protocol::ToolProgress>,
        progress: Result<peritus_tool_protocol::ToolProgress, peritus_tool_protocol::ProtocolError>,
    ) -> Result<(), DispatchFailure> {
        if self.next_progress >= self.prepared.call().limits().progress_events() {
            self.progress_truncated = true;
            return Ok(());
        }
        let progress =
            progress.map_err(|error| failure::adapter("shell-progress", &error.to_string()))?;
        self.next_progress = self.next_progress.saturating_add(1);
        target.push(progress);
        Ok(())
    }

    fn finalize(&mut self, observed_at: AuthorityInstant) -> Result<ToolResult, DispatchFailure> {
        let owner = self.owner.take().ok_or_else(|| {
            failure::adapter("shell-owner-missing", "owned process was already consumed")
        })?;
        let terminal = match owner.wait_and_publish(&self.artifacts, self.creating_event) {
            Ok(terminal) => terminal,
            Err(error) => error
                .terminal_result()
                .cloned()
                .ok_or_else(|| failure::process(error.process_error()))?,
        };
        let retained = self.control.retained_output();
        let result = terminal::build(
            &self.prepared,
            &terminal,
            &retained,
            self.started_at.unwrap_or(observed_at),
            observed_at,
            self.next_progress,
            self.progress_truncated,
        )?;
        self.terminal = Some(result.clone());
        Ok(result)
    }

    fn apply_control(&self, control: ToolControl) -> Result<(), DispatchFailure> {
        match control {
            ToolControl::Poll => Ok(()),
            ToolControl::Stdin(bytes) => {
                self.control.write_stdin(bytes).map_err(|error| failure::process(&error))
            }
            ToolControl::Resize { rows, columns } => {
                let size = TerminalSize::new(rows, columns, 0, 0)
                    .map_err(|error| failure::process(&error))?;
                self.control.resize(size).map_err(|error| failure::process(&error))
            }
            ToolControl::Signal(name) => {
                let signal = match name.as_str() {
                    "INT" | "SIGINT" | "interrupt" => ProcessSignal::Interrupt,
                    "TERM" | "SIGTERM" | "terminate" => ProcessSignal::Terminate,
                    _ => {
                        return Err(failure::adapter(
                            "shell-unsupported-signal",
                            "signal must be INT, SIGINT, TERM, SIGTERM, interrupt, or terminate",
                        ));
                    }
                };
                self.control.signal(signal).map_err(|error| failure::process(&error))
            }
            ToolControl::Cancel(reason) => {
                self.control.cancel(cancellation(reason)).map_err(|error| failure::process(&error))
            }
        }
    }
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
        self.apply_control(control)?;
        self.poll_owned(observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if self.control.terminal_result().is_none() {
            self.control.cancel(cancellation(reason)).map_err(|error| failure::process(&error))?;
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
}

const fn cancellation(reason: CancellationReason) -> ProcessCancellation {
    match reason {
        CancellationReason::Requested => ProcessCancellation::User,
        CancellationReason::Deadline => ProcessCancellation::Deadline,
        CancellationReason::Shutdown => ProcessCancellation::SupervisorShutdown,
        CancellationReason::Recovery => ProcessCancellation::BackendFailure,
    }
}
