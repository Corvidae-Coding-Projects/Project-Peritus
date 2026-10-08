//! Active quality execution observation, cancellation, and recovery.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_policy::AuthorityInstant;
use peritus_process::{
    CancellationReason as ProcessCancellation, OutputCompleteness, OutputStream, OwnedProcess,
    ExecutionGateway, ProcessControl, ProcessCursor, ProcessStore, RetainedOwnerRequest,
    TerminalResult,
};
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ToolControl, ToolResult,
};
use peritus_tool_router::{DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution};
use peritus_types::{EventId, ProcessId};

use super::{failure, progress, terminal};
use crate::{CheckDefinition, checkpoint, dispatcher::adapter_failure, parser};

const EVENT_PAGE: usize = 256;
const PARSER_READ_PAGE: usize = 32 * 1_024;

pub struct QualityExecution {
    prepared: PreparedToolCall,
    definition: CheckDefinition,
    owner: Option<OwnedProcess>,
    control: Option<ProcessControl>,
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
    settlement: Option<checkpoint::Settlement>,
    checkpoint: checkpoint::Owner,
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
        checkpoint: checkpoint::Owner,
    ) -> Self {
        let control = owner.control();
        Self {
            prepared,
            definition,
            owner: Some(owner),
            control: Some(control),
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
            settlement: None,
            checkpoint,
        }
    }

    /// Reconstructs the exact accepted execution from its protected checkpoint and C2 owner.
    ///
    /// `accepted_progress` must come from the router's durable progress chain. A checkpointed
    /// pending page is promoted only when that chain proves it was accepted. This method never
    /// dispatches a process or reuses the original execution authority.
    ///
    /// # Errors
    /// Rejects missing, corrupt, mismatched, regressed, or non-adoptable execution state.
    pub fn adopt(
        gateway: &ExecutionGateway,
        prepared: PreparedToolCall,
        artifacts: ArtifactStore,
        accepted_progress: u64,
        observed_at: AuthorityInstant,
    ) -> Result<Self, DispatchFailure> {
        let process_store = gateway.store().clone();
        let (mut checkpoint, mut state) =
            checkpoint::Owner::recover(&process_store, &prepared, &artifacts)?;
        if observed_at.epoch() != state.last_observed_at.epoch()
            || observed_at.tick_millis() < state.last_observed_at.tick_millis()
        {
            return Err(adapter_failure(
                "quality-adoption-time",
                "quality adoption time regressed or crossed the checkpoint authority epoch",
            ));
        }
        let selected = if state.committed.next_progress == accepted_progress {
            state.committed
        } else if state
            .pending
            .is_some_and(|pending| pending.next_progress == accepted_progress)
        {
            let accepted = state.pending.expect("checked pending progress");
            state.committed = accepted;
            state.pending = None;
            checkpoint.save(&state)?;
            accepted
        } else {
            return Err(adapter_failure(
                "quality-adoption-frontier",
                "durable router progress differs from committed and pending quality frontiers",
            ));
        };
        let process_id = checkpoint.process_id();
        let terminal = process_store.terminal_result(process_id).ok();
        let (owner, control, completed) = match terminal {
            Some(terminal) => {
                validate_terminal_binding(&checkpoint, &terminal)?;
                (None, None, Some(CompletedProcessEvidence { terminal }))
            }
            None => {
                validate_retained_binding(&process_store, &checkpoint, &prepared)?;
                let owner = gateway
                    .reattach_retained(process_id)
                    .map_err(|error| failure::process(&error))?;
                let control = owner.control();
                let completed = control
                    .terminal_result()
                    .map(|terminal| CompletedProcessEvidence { terminal });
                (Some(owner), Some(control), completed)
            }
        };
        let definition = checkpoint.definition().clone();
        let creating_event = checkpoint.creating_event();
        let started_at = checkpoint.started_at();
        let mut execution = Self {
            prepared,
            definition,
            owner,
            control,
            process_store,
            process_id,
            artifacts,
            creating_event,
            cursor: selected.cursor,
            next_progress: selected.next_progress,
            started_at,
            last_observed_at: state.last_observed_at,
            progress_truncated: selected.progress_truncated,
            completed,
            result: None,
            pending_progress: None,
            settlement: state.settlement,
            checkpoint,
        };
        if execution.control.is_none()
            && execution.settlement.is_none_or(|settlement| {
                settlement.progress_frontier != accepted_progress
            })
        {
            execution.progress_truncated = true;
        }
        execution.restore_settled_result(accepted_progress)?;
        Ok(execution)
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.validate_time(observed_at)?;
        if self.pending_progress.is_some() {
            let update = self
                .pending_progress
                .as_ref()
                .expect("checked pending progress")
                .update
                .clone();
            self.persist_checkpoint()?;
            return Ok(update);
        }
        if self.result.is_some() {
            let result = self.result.as_ref().expect("checked quality result").clone();
            self.persist_checkpoint()?;
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result))
                .map_err(|error| adapter_failure("quality-terminal-repeat", &error.to_string()));
        }
        let mut cursor = self.cursor;
        let mut next_progress = self.next_progress;
        let mut progress_truncated = self.progress_truncated;
        let page_capacity = usize::try_from(self.prepared.call().limits().progress_events())
            .unwrap_or(usize::MAX)
            .min(EVENT_PAGE);
        let mut progress_updates = Vec::with_capacity(page_capacity);
        let progress_observed_at = self
            .settlement
            .filter(|settlement| settlement.progress_frontier > self.next_progress)
            .map_or(observed_at, |settlement| settlement.finished_at);
        if next_progress == 0 && progress_updates.len() < page_capacity {
            progress_updates.push(
                progress::started(&self.prepared, 0, progress_observed_at)
                    .map_err(|error| adapter_failure("quality-progress", &error.to_string()))?,
            );
            next_progress = 1;
        }
        let events = self.control.as_ref().map_or_else(Vec::new, |control| {
            control.read_events(cursor, page_capacity.saturating_sub(progress_updates.len()))
        });
        for event in &events {
            if event.loss().is_some()
                || event.sequence() != cursor.sequence().saturating_add(1)
            {
                progress_truncated = true;
            }
            cursor = ProcessCursor::after_event(event);
            progress_updates.push(
                progress::event(&self.prepared, next_progress, event, progress_observed_at)
                    .map_err(|error| adapter_failure("quality-progress", &error.to_string()))?,
            );
            next_progress = next_progress.checked_add(1).ok_or_else(|| {
                adapter_failure("quality-progress-frontier", "progress frontier overflowed")
            })?;
        }
        if self
            .settlement
            .is_some_and(|settlement| next_progress > settlement.progress_frontier)
        {
            self.settlement = None;
        }
        let has_more = self
            .control
            .as_ref()
            .is_some_and(|control| !control.read_events(cursor, 1).is_empty());
        let terminal = if !has_more
            && (self.completed.is_some()
                || self.control.as_ref().is_some_and(|control| {
                    control.terminal_result().is_some() || control.owner_finished()
                }))
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
                    self.persist_checkpoint()?;
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
        self.persist_checkpoint()?;
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
        let progress_truncated = progress_truncated
            || completed.terminal.output().event_records_dropped() > 0;
        let parsed = parser::parse(
            self.definition.parser(),
            &parser_input.bytes,
            parser_input.completeness,
            parser_input.exact,
        );
        let predicate_satisfied =
            parsed.as_ref().is_ok_and(parser::ParsedOutput::predicate_satisfied);
        let parser_complete = parsed.is_ok();
        let settlement = match self.settlement {
            Some(settlement) if settlement.progress_frontier == progress_frontier => {
                if settlement.parser_complete != parser_complete
                    || settlement.predicate_satisfied != predicate_satisfied
                    || settlement.progress_truncated != progress_truncated
                {
                    return Err(adapter_failure(
                        "quality-settlement-checkpoint",
                        "recomputed quality settlement differs from its durable parser receipt",
                    ));
                }
                settlement
            }
            _ => checkpoint::Settlement {
                finished_at: observed_at,
                progress_frontier,
                progress_truncated,
                parser_complete,
                predicate_satisfied,
            },
        };
        let terminal = terminal::build(
            &self.prepared,
            &self.definition,
            &completed.terminal,
            parser_complete,
            predicate_satisfied,
            &self.artifacts,
            self.started_at,
            settlement.finished_at,
            progress_frontier,
            progress_truncated,
        )?;
        self.settlement = Some(settlement);
        self.result = Some(terminal.clone());
        Ok(terminal)
    }

    fn restore_settled_result(
        &mut self,
        accepted_progress: u64,
    ) -> Result<(), DispatchFailure> {
        let Some(settlement) = self
            .settlement
            .filter(|settlement| settlement.progress_frontier == accepted_progress)
        else {
            return Ok(());
        };
        self.capture_terminal()?;
        self.complete_artifact_publication()?;
        let parser_input = self.parser_input()?;
        let parsed = parser::parse(
            self.definition.parser(),
            &parser_input.bytes,
            parser_input.completeness,
            parser_input.exact,
        );
        let parser_complete = parsed.is_ok();
        let predicate_satisfied =
            parsed.as_ref().is_ok_and(parser::ParsedOutput::predicate_satisfied);
        if parser_complete != settlement.parser_complete
            || predicate_satisfied != settlement.predicate_satisfied
        {
            return Err(adapter_failure(
                "quality-settlement-checkpoint",
                "adopted parser result differs from its durable settlement receipt",
            ));
        }
        let completed = self.completed.as_ref().ok_or_else(|| {
            adapter_failure("quality-settlement-missing", "adopted terminal evidence disappeared")
        })?;
        self.result = Some(terminal::build(
            &self.prepared,
            &self.definition,
            &completed.terminal,
            settlement.parser_complete,
            settlement.predicate_satisfied,
            &self.artifacts,
            self.started_at,
            settlement.finished_at,
            settlement.progress_frontier,
            settlement.progress_truncated,
        )?);
        Ok(())
    }

    fn persist_checkpoint(&mut self) -> Result<(), DispatchFailure> {
        self.checkpoint.save(&checkpoint::State {
            last_observed_at: self.last_observed_at,
            committed: checkpoint::CursorState {
                cursor: self.cursor,
                next_progress: self.next_progress,
                progress_truncated: self.progress_truncated,
            },
            pending: self.pending_progress.as_ref().map(|pending| checkpoint::CursorState {
                cursor: pending.cursor,
                next_progress: pending.next_progress,
                progress_truncated: pending.progress_truncated,
            }),
            settlement: self.settlement,
        })
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
                        .or_else(|| self.control.as_ref().and_then(ProcessControl::terminal_result))
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
            .as_ref()
            .and_then(ProcessControl::terminal_result)
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
                    .or_else(|| self.control.as_ref().and_then(ProcessControl::terminal_result))
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
        self.completed = Some(CompletedProcessEvidence { terminal });
        Ok(())
    }

    fn request_cancellation(&self, reason: CancellationReason) -> Result<(), DispatchFailure> {
        let Some(control) = &self.control else {
            return if self.completed.is_some() {
                Ok(())
            } else {
                Err(adapter_failure(
                    "quality-control-owner",
                    "quality execution has no adoptable process control owner",
                ))
            };
        };
        if control.terminal_result().is_some() || control.owner_finished() {
            return Ok(());
        }
        match control.cancel(cancellation(reason)) {
            Ok(()) => Ok(()),
            Err(_)
                if control.terminal_result().is_some() || control.owner_finished() =>
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
        let pending = self.pending_progress.as_ref().expect("checked pending progress");
        self.checkpoint.save(&checkpoint::State {
            last_observed_at: self.last_observed_at,
            committed: checkpoint::CursorState {
                cursor: pending.cursor,
                next_progress: pending.next_progress,
                progress_truncated: pending.progress_truncated,
            },
            pending: None,
            settlement: self.settlement,
        })?;
        let pending = self.pending_progress.take().expect("checked pending progress");
        self.cursor = pending.cursor;
        self.next_progress = pending.next_progress;
        self.progress_truncated = pending.progress_truncated;
        Ok(())
    }
}

fn validate_terminal_binding(
    checkpoint: &checkpoint::Owner,
    terminal: &TerminalResult,
) -> Result<(), DispatchFailure> {
    if terminal.process_id() != checkpoint.process_id()
        || terminal.plan_digest() != checkpoint.plan_digest()
    {
        return Err(adapter_failure(
            "quality-adoption-process",
            "durable terminal evidence differs from the checkpointed process owner",
        ));
    }
    Ok(())
}

fn validate_retained_binding(
    store: &ProcessStore,
    checkpoint: &checkpoint::Owner,
    prepared: &PreparedToolCall,
) -> Result<(), DispatchFailure> {
    let reservation = store
        .retained_owner_reservation(checkpoint.process_id())
        .map_err(|error| failure::process(&error))?
        .ok_or_else(|| {
            adapter_failure(
                "quality-adoption-owner",
                "checkpointed active process has no retained C2 owner request",
            )
        })?;
    let request = RetainedOwnerRequest::decode(reservation.request().to_vec())
        .map_err(|error| failure::process(&error))?;
    let plan = request.execution_plan();
    let caller = plan.caller_binding().ok_or_else(|| {
        adapter_failure(
            "quality-adoption-owner",
            "retained quality process has no C4 caller binding",
        )
    })?;
    if request.binding().process_id() != checkpoint.process_id()
        || request.binding().execution_plan_digest() != checkpoint.plan_digest()
        || plan.digest() != checkpoint.plan_digest()
        || caller.action_id() != prepared.call().action_id()
        || caller.capability_name() != prepared.descriptor().name()
        || caller.descriptor_digest() != prepared.descriptor_digest().get()
        || caller.prepared_digest() != prepared.prepared_digest()
    {
        return Err(adapter_failure(
            "quality-adoption-owner",
            "retained C2 owner differs from the checkpointed quality invocation",
        ));
    }
    Ok(())
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
