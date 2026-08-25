//! Active quality execution observation, cancellation, and recovery.

use peritus_artifact_store::ArtifactStore;
use peritus_policy::AuthorityInstant;
use peritus_process::{
    CancellationReason as ProcessCancellation, OutputCompleteness, OutputStream, OwnedProcess,
    ProcessControl, ProcessCursor, ProcessEventKind,
};
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ToolControl, ToolProgress, ToolResult,
};
use peritus_tool_router::{DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution};
use peritus_types::EventId;

use super::{failure, progress, terminal};
use crate::{CheckDefinition, dispatcher::adapter_failure, parser};

const EVENT_PAGE: usize = 256;

pub struct QualityExecution {
    prepared: PreparedToolCall,
    definition: CheckDefinition,
    owner: Option<OwnedProcess>,
    control: ProcessControl,
    artifacts: ArtifactStore,
    creating_event: EventId,
    cursor: ProcessCursor,
    next_progress: u32,
    started_at: AuthorityInstant,
    last_observed_at: AuthorityInstant,
    progress_truncated: bool,
    parser_sequence_complete: bool,
    parser_overflow: bool,
    parser_output: Vec<u8>,
    result: Option<ToolResult>,
}

impl QualityExecution {
    pub(crate) fn new(
        prepared: PreparedToolCall,
        definition: CheckDefinition,
        owner: OwnedProcess,
        artifacts: ArtifactStore,
        creating_event: EventId,
        started_at: AuthorityInstant,
    ) -> Self {
        let control = owner.control();
        Self {
            prepared,
            definition,
            owner: Some(owner),
            control,
            artifacts,
            creating_event,
            cursor: ProcessCursor::after(0),
            next_progress: 0,
            started_at,
            last_observed_at: started_at,
            progress_truncated: false,
            parser_sequence_complete: true,
            parser_overflow: false,
            parser_output: Vec::new(),
            result: None,
        }
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.validate_time(observed_at)?;
        if let Some(result) = &self.result {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|error| adapter_failure("quality-terminal-repeat", &error.to_string()));
        }
        let mut progress_updates = Vec::new();
        if self.next_progress == 0 {
            self.push_progress(
                &mut progress_updates,
                progress::started(&self.prepared, 0, observed_at),
            )?;
        }
        let events = self.control.read_events(self.cursor, EVENT_PAGE);
        for event in &events {
            if event.sequence() != self.cursor.sequence().saturating_add(1) {
                self.parser_sequence_complete = false;
                self.progress_truncated = true;
            }
            self.cursor = ProcessCursor::after(event.sequence());
            self.capture(event.kind(), event.data());
            let projected = progress::event(&self.prepared, self.next_progress, event, observed_at);
            self.push_progress(&mut progress_updates, projected)?;
        }
        let terminal = if events.len() < EVENT_PAGE && self.control.terminal_result().is_some() {
            Some(self.finalize(observed_at)?)
        } else {
            None
        };
        ExecutionUpdate::new(&self.prepared, progress_updates, terminal)
            .map_err(|error| adapter_failure("quality-progress-envelope", &error.to_string()))
    }

    fn validate_time(&mut self, observed_at: AuthorityInstant) -> Result<(), DispatchFailure> {
        if observed_at.epoch() != self.last_observed_at.epoch()
            || observed_at.tick_millis() < self.last_observed_at.tick_millis()
        {
            return Err(adapter_failure(
                "quality-observation-time",
                "authority observation time regressed or crossed epochs",
            ));
        }
        self.last_observed_at = observed_at;
        Ok(())
    }

    fn capture(&mut self, kind: &ProcessEventKind, data: &[u8]) {
        let relevant =
            matches!(kind, ProcessEventKind::Output(OutputStream::Stdout | OutputStream::Terminal));
        let Some(maximum) = self.definition.parser().maximum_bytes() else { return };
        if relevant {
            if self.parser_output.len().saturating_add(data.len()) > maximum as usize {
                self.parser_overflow = true;
            } else {
                self.parser_output.extend_from_slice(data);
            }
        }
    }

    fn push_progress(
        &mut self,
        target: &mut Vec<ToolProgress>,
        item: Result<ToolProgress, peritus_tool_protocol::ProtocolError>,
    ) -> Result<(), DispatchFailure> {
        if self.next_progress >= self.prepared.call().limits().progress_events() {
            self.progress_truncated = true;
            return Ok(());
        }
        target.push(item.map_err(|error| adapter_failure("quality-progress", &error.to_string()))?);
        self.next_progress = self.next_progress.saturating_add(1);
        Ok(())
    }

    fn finalize(&mut self, observed_at: AuthorityInstant) -> Result<ToolResult, DispatchFailure> {
        let owner = self.owner.take().ok_or_else(|| {
            adapter_failure("quality-owner-missing", "owned quality process was already consumed")
        })?;
        let result = match owner.wait_and_publish(&self.artifacts, self.creating_event) {
            Ok(result) => result,
            Err(error) => error
                .terminal_result()
                .cloned()
                .ok_or_else(|| failure::process(error.process_error()))?,
        };
        let retained = self.control.retained_output();
        let completeness = parser_stream_completeness(&result);
        let parsed = parser::parse(
            self.definition.parser(),
            &self.parser_output,
            completeness,
            self.parser_sequence_complete && !self.parser_overflow,
        );
        let predicate_satisfied =
            parsed.as_ref().is_ok_and(parser::ParsedOutput::predicate_satisfied);
        let terminal = terminal::build(
            &self.prepared,
            &self.definition,
            &result,
            parsed.is_ok(),
            predicate_satisfied,
            &retained,
            self.started_at,
            observed_at,
            self.next_progress,
            self.progress_truncated,
        )?;
        self.result = Some(terminal.clone());
        Ok(terminal)
    }
}

impl ToolExecution for QualityExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll_owned(observed_at)
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        match control {
            ToolControl::Poll => {}
            ToolControl::Cancel(reason) => self
                .control
                .cancel(cancellation(reason))
                .map_err(|error| failure::process(&error))?,
            _ => {
                return Err(adapter_failure(
                    "quality-control-unsupported",
                    "quality runs support only poll and cancellation",
                ));
            }
        }
        self.poll_owned(observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.control.cancel(cancellation(reason)).map_err(|error| failure::process(&error))?;
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

fn parser_stream_completeness(result: &peritus_process::TerminalResult) -> OutputCompleteness {
    result
        .output()
        .streams()
        .iter()
        .find(|stream| matches!(stream.stream(), OutputStream::Stdout | OutputStream::Terminal))
        .map_or(OutputCompleteness::Incomplete, |stream| stream.completeness())
}

const fn cancellation(reason: CancellationReason) -> ProcessCancellation {
    match reason {
        CancellationReason::Requested => ProcessCancellation::User,
        CancellationReason::Deadline => ProcessCancellation::Deadline,
        CancellationReason::Shutdown => ProcessCancellation::SupervisorShutdown,
        CancellationReason::Recovery => ProcessCancellation::BackendFailure,
    }
}
