//! Instrumented production dispatcher/execution adapters.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use peritus_conformance::ToolEffectObservation;
use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CancellationReason, ImplementationIdentity, PreparedToolCall,
    ProgressKind, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, SchemaDigest,
    ToolControl, ToolFailure, ToolProgress, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolDispatcher,
    ToolExecution, ToolStart,
};

#[derive(Default)]
pub struct Counters {
    starts: AtomicU64,
    effects: AtomicU64,
    control: AtomicBool,
    joined: AtomicBool,
}

impl Counters {
    pub fn effects(&self) -> ToolEffectObservation {
        let starts = self.starts.load(Ordering::SeqCst);
        ToolEffectObservation::new(starts, starts, starts, self.effects.load(Ordering::SeqCst))
    }

    pub fn control_observed(&self) -> bool {
        self.control.load(Ordering::SeqCst)
    }

    pub fn joined(&self) -> bool {
        self.joined.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy)]
pub enum SyncMode {
    Success,
    Failure,
}

pub struct SyncDispatcher {
    identity: ImplementationIdentity,
    digest: SchemaDigest,
    mode: SyncMode,
    counters: Arc<Counters>,
}

impl SyncDispatcher {
    pub fn new(prepared: &PreparedToolCall, mode: SyncMode, counters: Arc<Counters>) -> Self {
        Self {
            identity: prepared.descriptor().implementation_identity().clone(),
            digest: prepared.descriptor_digest(),
            mode,
            counters,
        }
    }
}

impl ToolDispatcher for SyncDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        self.counters.starts.fetch_add(1, Ordering::SeqCst);
        self.counters.effects.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            SyncMode::Success => {
                let result = success(
                    invocation.prepared(),
                    invocation.observed_at(),
                    invocation.observed_at(),
                    0,
                );
                Ok(ToolStart::Completed(result))
            }
            SyncMode::Failure => Err(failure(ResultStatus::Failed, "read_failed")),
        }
    }
}

#[derive(Clone, Copy)]
pub enum ActiveMode {
    Cancel,
    Deadline,
    Lost,
}

pub struct ActiveDispatcher {
    identity: ImplementationIdentity,
    digest: SchemaDigest,
    mode: ActiveMode,
    counters: Arc<Counters>,
}

impl ActiveDispatcher {
    pub fn new(prepared: &PreparedToolCall, mode: ActiveMode, counters: Arc<Counters>) -> Self {
        Self {
            identity: prepared.descriptor().implementation_identity().clone(),
            digest: prepared.descriptor_digest(),
            mode,
            counters,
        }
    }
}

impl ToolDispatcher for ActiveDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        self.counters.starts.fetch_add(1, Ordering::SeqCst);
        self.counters.effects.fetch_add(1, Ordering::SeqCst);
        let started_at = invocation.observed_at();
        Ok(ToolStart::Active(Box::new(ActiveExecution {
            prepared: invocation.into_prepared(),
            started_at,
            mode: self.mode,
            counters: Arc::clone(&self.counters),
        })))
    }
}

struct ActiveExecution {
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    mode: ActiveMode,
    counters: Arc<Counters>,
}

impl Drop for ActiveExecution {
    fn drop(&mut self) {
        self.counters.joined.store(true, Ordering::SeqCst);
    }
}

impl ToolExecution for ActiveExecution {
    fn poll(&mut self, _observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        ExecutionUpdate::new(&self.prepared, Vec::new(), None)
            .map_err(|_| failure(ResultStatus::Indeterminate, "invalid_active_poll"))
    }

    fn control(
        &mut self,
        _control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.cancel(CancellationReason::Requested, observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.counters.control.store(true, Ordering::SeqCst);
        let (status, code) = match (self.mode, reason) {
            (ActiveMode::Deadline, _) | (_, CancellationReason::Deadline) => {
                (ResultStatus::TimedOut, "deadline")
            }
            _ => (ResultStatus::Cancelled, "cancelled"),
        };
        let progress = ToolProgress::new(
            &self.prepared,
            0,
            ProgressKind::Stopping,
            observed_at,
            None,
            BoundedText::new(code.to_owned()).unwrap(),
        )
        .unwrap();
        let result = ToolResult::failure(
            &self.prepared,
            status,
            failure(status, code).failure().clone(),
            None,
            BoundedText::new(code.to_owned()).unwrap(),
            BoundedText::new(code.to_owned()).unwrap(),
            Vec::new(),
            ToolTiming::new(self.started_at, observed_at).unwrap(),
            complete_truncation(),
            1,
        )
        .unwrap();
        Ok(ExecutionUpdate::new(&self.prepared, vec![progress], Some(result)).unwrap())
    }

    fn recover(
        &mut self,
        _observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        match self.mode {
            ActiveMode::Lost => {
                Ok(RecoveryObservation::Lost(failure(ResultStatus::Indeterminate, "lost")))
            }
            ActiveMode::Cancel | ActiveMode::Deadline => Ok(RecoveryObservation::Active(
                ExecutionUpdate::new(&self.prepared, Vec::new(), None).unwrap(),
            )),
        }
    }
}

pub fn failure(status: ResultStatus, code: &str) -> DispatchFailure {
    DispatchFailure::new(
        status,
        ToolFailure::new(
            peritus_tool_protocol::FailureCategory::Execution,
            BoundedText::new(code.to_owned()).unwrap(),
            ResponsibleSubsystem::Tool,
            Retryability::Never,
            RecoveryRoute::None,
            BoundedText::new(code.to_owned()).unwrap(),
        ),
    )
    .unwrap()
}

fn success(
    prepared: &PreparedToolCall,
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u32,
) -> ToolResult {
    ToolResult::success(
        prepared,
        BoundedJson::null(),
        BoundedText::new("ok".to_owned()).unwrap(),
        BoundedText::new("ok".to_owned()).unwrap(),
        Vec::new(),
        ToolTiming::new(started_at, finished_at).unwrap(),
        complete_truncation(),
        progress_count,
    )
    .unwrap()
}

const fn complete_truncation() -> TruncationMetadata {
    TruncationMetadata {
        output: Truncation::Complete,
        model: Truncation::Complete,
        human: Truncation::Complete,
    }
}
