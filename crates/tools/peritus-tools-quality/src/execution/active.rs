//! Active quality execution observation, cancellation, and recovery.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_policy::AuthorityInstant;
use peritus_process::{
    CancellationReason as ProcessCancellation, OutputCompleteness, OutputStream, OwnedProcess,
    ProcessControl, ProcessCursor, ProcessStore, TerminalResult,
};
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ToolControl, ToolResult,
};
use peritus_tool_router::{DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution};
use peritus_types::{EventId, ProcessId};

use super::{failure, progress, terminal};
use crate::{CheckDefinition, dispatcher::adapter_failure, parser};

const EVENT_PAGE: usize = 256;
const PARSER_READ_PAGE: usize = 32 * 1_024;

pub struct QualityExecution {
    prepared: PreparedToolCall,
    definition: CheckDefinition,
    owner: Option<OwnedProcess>,
    control: ProcessControl,
    process_store: ProcessStore,
    process_id: ProcessId,
    artifacts: ArtifactStore,
    creating_event: EventId,
    cursor: ProcessCursor,
    next_progress: u64,
    started_at: AuthorityInstant,
    last_observed_at: AuthorityInstant,
    progress_truncated: bool,
    completed: Option<CompletedProcessEvidence>,
    result: Option<ToolResult>,
    pending_progress: Option<PendingProgress>,
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

struct ParserInput {
    bytes: Vec<u8>,
    completeness: OutputCompleteness,
    exact: bool,
}

impl QualityExecution {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        prepared: PreparedToolCall,
        definition: CheckDefinition,
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
            definition,
            owner: Some(owner),
            control,
            process_store,
            process_id,
            artifacts,
            creating_event,
            cursor: ProcessCursor::after(0),
            next_progress: 0,
            started_at,
            last_observed_at: started_at,
            progress_truncated: false,
            completed: None,
            result: None,
            pending_progress: None,
        }
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.validate_time(observed_at)?;
        if let Some(pending) = &self.pending_progress {
            return Ok(pending.update.clone());
        }
        if let Some(result) = &self.result {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|error| adapter_failure("quality-terminal-repeat", &error.to_string()));
        }
        let mut cursor = self.cursor;
        let mut next_progress = self.next_progress;
        let mut progress_truncated = self.progress_truncated;
        let page_capacity = usize::try_from(self.prepared.call().limits().progress_events())
            .unwrap_or(usize::MAX)
            .min(EVENT_PAGE);
        let mut progress_updates = Vec::with_capacity(page_capacity);
        if next_progress == 0 && progress_updates.len() < page_capacity {
            progress_updates.push(
                progress::started(&self.prepared, 0, observed_at)
                    .map_err(|error| adapter_failure("quality-progress", &error.to_string()))?,
            );
            next_progress = 1;
        }
        let events = self
            .control
            .read_events(cursor, page_capacity.saturating_sub(progress_updates.len()));
        for event in &events {
            if event.loss().is_some()
                || event.sequence() != cursor.sequence().saturating_add(1)
            {
                progress_truncated = true;
            }
            cursor = ProcessCursor::after_event(event);
            progress_updates.push(
                progress::event(&self.prepared, next_progress, event, observed_at)
                    .map_err(|error| adapter_failure("quality-progress", &error.to_string()))?,
            );
            next_progress = next_progress.checked_add(1).ok_or_else(|| {
                adapter_failure("quality-progress-frontier", "progress frontier overflowed")
            })?;
        }
        let has_more = !self.control.read_events(cursor, 1).is_empty();
        let terminal = if !has_more
            && (self.completed.is_some()
                || self.control.terminal_result().is_some()
                || self.control.owner_finished())
        {
            match self.finalize(
                observed_at,
                next_progress,
                progress_truncated,
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    let update = ExecutionUpdate::settlement_pending(
                        &self.prepared,
                        progress_updates,
                        failure::settlement_projection(&error),
                    )
                    .map_err(|error| {
                        adapter_failure("quality-settlement-envelope", &error.to_string())
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
        let update = ExecutionUpdate::new(&self.prepared, progress_updates, terminal)
            .map_err(|error| adapter_failure("quality-progress-envelope", &error.to_string()))?;
        self.pending_progress = Some(PendingProgress {
            update: update.clone(),
            cursor,
            next_progress,
            progress_truncated,
        });
        Ok(update)
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

    fn finalize(
        &mut self,
        observed_at: AuthorityInstant,
        progress_frontier: u64,
        progress_truncated: bool,
    ) -> Result<ToolResult, DispatchFailure> {
        self.capture_terminal()?;
        self.complete_artifact_publication()?;
        let parser_input = self.parser_input()?;
        let completed = self.completed.as_ref().ok_or_else(|| {
            adapter_failure("quality-settlement-missing", "completed process evidence disappeared")
        })?;
        let parsed = parser::parse(
            self.definition.parser(),
            &parser_input.bytes,
            parser_input.completeness,
            parser_input.exact,
        );
        let predicate_satisfied =
            parsed.as_ref().is_ok_and(parser::ParsedOutput::predicate_satisfied);
        let terminal = terminal::build(
            &self.prepared,
            &self.definition,
            &completed.terminal,
            parsed.is_ok(),
            predicate_satisfied,
            &completed.retained_output,
            self.started_at,
            observed_at,
            progress_frontier,
            progress_truncated,
        )?;
        self.result = Some(terminal.clone());
        Ok(terminal)
    }

    fn parser_input(&self) -> Result<ParserInput, DispatchFailure> {
        let Some(maximum) = self.definition.parser().maximum_bytes() else {
            return Ok(ParserInput {
                bytes: Vec::new(),
                completeness: OutputCompleteness::Complete,
                exact: true,
            });
        };
        let completed = self.completed.as_ref().ok_or_else(|| {
            adapter_failure("quality-parser-artifact", "parser has no completed process evidence")
        })?;
        let Some(accounting) = parser_stream(&completed.terminal) else {
            return Ok(ParserInput {
                bytes: Vec::new(),
                completeness: OutputCompleteness::Incomplete,
                exact: false,
            });
        };
        let completeness = accounting.completeness();
        if completeness != OutputCompleteness::Complete
            || accounting.retained() != accounting.observed()
            || accounting.retained() > u64::from(maximum)
        {
            return Ok(ParserInput { bytes: Vec::new(), completeness, exact: false });
        }
        if accounting.retained() == 0 {
            return Ok(ParserInput { bytes: Vec::new(), completeness, exact: true });
        }
        let artifact = completed
            .terminal
            .artifacts()
            .iter()
            .find(|artifact| artifact.stream() == accounting.stream())
            .ok_or_else(|| {
                adapter_failure(
                    "quality-parser-artifact",
                    "complete parser stream has no published output artifact",
                )
            })?;
        if artifact.start_offset() != 0
            || artifact.end_offset() != artifact.size()
            || artifact.size() != accounting.retained()
            || artifact.completeness() != completeness
        {
            return Err(adapter_failure(
                "quality-parser-artifact",
                "published parser artifact range differs from C2 terminal accounting",
            ));
        }
        let digest = ArtifactDigest::from_sha256(artifact.digest());
        let mut reader = self.artifacts.open_read(digest).map_err(|error| {
            failure::artifact(&error)
        })?;
        if reader.metadata().digest() != digest || reader.metadata().size() != artifact.size() {
            return Err(adapter_failure(
                "quality-parser-artifact",
                "opened parser artifact identity differs from C2 terminal evidence",
            ));
        }
        let expected = usize::try_from(artifact.size()).map_err(|_| {
            adapter_failure(
                "quality-parser-artifact",
                "parser artifact size exceeds native addressability",
            )
        })?;
        let mut bytes = Vec::with_capacity(expected);
        while bytes.len() < expected {
            let remaining = expected - bytes.len();
            let chunk = reader
                .read_chunk(remaining.min(PARSER_READ_PAGE))
                .map_err(|error| failure::artifact(&error))?
                .ok_or_else(|| {
                    adapter_failure(
                        "quality-parser-artifact",
                        "parser artifact ended before its authenticated range",
                    )
                })?;
            let expected_offset = u64::try_from(bytes.len()).map_err(|_| {
                adapter_failure(
                    "quality-parser-artifact",
                    "parser artifact page offset exceeds its integer representation",
                )
            })?;
            if chunk.offset() != expected_offset {
                return Err(adapter_failure(
                    "quality-parser-artifact",
                    "parser artifact page offset is not contiguous",
                ));
            }
            bytes.extend_from_slice(chunk.bytes());
        }
        Ok(ParserInput { bytes, completeness, exact: true })
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
                        if self.publication_complete() {
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
        if self.publication_complete() {
            return Ok(());
        }
        match self.process_store.retry_artifact_publication(
            self.process_id,
            &self.artifacts,
            self.creating_event,
        ) {
            Ok(terminal) => {
                self.retain_completed(terminal)?;
                if self.publication_complete() {
                    Ok(())
                } else {
                    Err(adapter_failure(
                        "quality-artifact-settlement",
                        "artifact publication returned without complete durable terminal evidence",
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
                    if self.publication_complete() {
                        return Ok(());
                    }
                }
                Err(failure)
            }
        }
    }

    fn publication_complete(&self) -> bool {
        self.completed.as_ref().is_some_and(|completed| {
            completed.terminal.artifact_publication_complete()
        })
    }

    fn retain_completed(&mut self, terminal: TerminalResult) -> Result<(), DispatchFailure> {
        if terminal.process_id() != self.process_id {
            return Err(adapter_failure(
                "quality-settlement-identity",
                "terminal evidence belongs to another process",
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
            Err(error) => Err(failure::process(&error)),
        }
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
            ToolControl::Cancel(reason) => {
                self.request_cancellation(reason)?;
            }
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
        self.request_cancellation(reason)?;
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
            adapter_failure(
                "quality-progress-ack",
                "router acknowledged progress without a retained pending page",
            )
        })?;
        if pending.next_progress != next_frontier {
            return Err(adapter_failure(
                "quality-progress-ack",
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

fn parser_stream(
    result: &peritus_process::TerminalResult,
) -> Option<&peritus_process::StreamAccounting> {
    result
        .output()
        .streams()
        .iter()
        .find(|stream| matches!(stream.stream(), OutputStream::Stdout | OutputStream::Terminal))
}

const fn cancellation(reason: CancellationReason) -> ProcessCancellation {
    match reason {
        CancellationReason::Requested => ProcessCancellation::User,
        CancellationReason::Deadline => ProcessCancellation::Deadline,
        CancellationReason::Shutdown => ProcessCancellation::SupervisorShutdown,
        CancellationReason::Recovery => ProcessCancellation::BackendFailure,
    }
}
