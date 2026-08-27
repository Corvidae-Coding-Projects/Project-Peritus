//! Authorized C2 launch, bounded birth observation, and owned PTY registration.

use std::time::Instant;

use peritus_process::{
    ExecutionAuthorizationRequest, ExecutionGateway, ExecutionPlan, OwnedProcess, ProcessControl,
    ProcessCursor, ProcessTreeIdentity,
};
use peritus_types::ProcessId;

use super::TerminalRegistry;
use crate::terminal::{
    TerminalBridgeError, TerminalBridgeErrorKind, bridge::LiveTerminalRegistration,
};

impl TerminalRegistry {
    /// Consumes exact C2 authority, launches one PTY, and retains its move-only lifecycle owner.
    ///
    /// The execution gateway remains the sole effect authority. This method only adds the daemon
    /// ownership and observation bridge required after C2 accepts the launch.
    ///
    /// # Errors
    ///
    /// Rejects a non-PTY plan before its authority can be consumed, propagates C2 launch failures,
    /// and cancels/joins a launched process if bounded birth registration fails.
    pub(crate) fn launch(
        &self,
        gateway: &ExecutionGateway,
        request: &ExecutionAuthorizationRequest<'_>,
        plan: ExecutionPlan,
    ) -> Result<ProcessId, TerminalBridgeError> {
        LiveTerminalRegistration::validate_plan(&plan)?;
        let owner = gateway.launch(request, plan.clone())?;
        self.register_owned(&plan, owner)
    }

    /// Registers an already-authorized C2 PTY while retaining its move-only lifecycle owner.
    ///
    /// This is the production seam for C3-backed launches, which also return an [`OwnedProcess`]
    /// after applying their native backend.
    ///
    /// # Errors
    ///
    /// Rejects a non-PTY plan, a missing or unsafe birth identity, a startup timeout, capacity
    /// exhaustion, or conflicting process registration.
    pub(crate) fn register_owned(
        &self,
        plan: &ExecutionPlan,
        owner: OwnedProcess,
    ) -> Result<ProcessId, TerminalBridgeError> {
        LiveTerminalRegistration::validate_plan(plan)?;
        let process_id = plan.identity().process_id();
        let control = owner.control();
        let limits = self.limits();
        let birth_identity = wait_for_birth_identity(
            &control,
            process_id,
            plan.digest(),
            limits.maximum_process_events_per_page(),
            limits.process_startup_wait(),
        )?;
        let registration = LiveTerminalRegistration::new(plan, birth_identity, owner)?;
        self.register(registration)?;
        Ok(process_id)
    }
}

fn wait_for_birth_identity(
    control: &ProcessControl,
    process_id: ProcessId,
    plan_digest: peritus_types::Sha256Digest,
    page_size: usize,
    timeout: std::time::Duration,
) -> Result<ProcessTreeIdentity, TerminalBridgeError> {
    let began = Instant::now();
    let mut cursor = ProcessCursor::after(0);
    loop {
        if let Some(identity) = control.tree_identity() {
            return Ok(identity);
        }
        if let Some(result) = control.terminal_result() {
            if result.process_id() != process_id || result.plan_digest() != plan_digest {
                return Err(rejected(
                    TerminalBridgeErrorKind::ProcessIdentityMismatch,
                    "pre-registration terminal result does not match the checked plan",
                ));
            }
            return Err(rejected(
                TerminalBridgeErrorKind::ProcessNotLive,
                "process reached a terminal result before publishing a live birth identity",
            ));
        }
        let Some(remaining) = timeout.checked_sub(began.elapsed()) else {
            return Err(rejected(
                TerminalBridgeErrorKind::BirthIdentityUnavailable,
                "process did not publish its birth identity within the configured bound",
            ));
        };
        if remaining.is_zero() {
            continue;
        }
        let events = control.wait_events(cursor, page_size, remaining);
        for event in &events {
            if event.process_id() != process_id || event.plan_digest() != plan_digest {
                return Err(rejected(
                    TerminalBridgeErrorKind::ProcessIdentityMismatch,
                    "pre-registration process event does not match the checked plan",
                ));
            }
        }
        if let Some(last) = events.last() {
            cursor = ProcessCursor::after(last.sequence());
        }
    }
}

const fn rejected(kind: TerminalBridgeErrorKind, detail: &'static str) -> TerminalBridgeError {
    TerminalBridgeError::rejected(kind, detail)
}
