//! Router-owned active entry validation and lifecycle helpers.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ResultStatus, ToolControl, ToolDescriptor, ToolResult,
};

use crate::{DispatchFailure, ExecutionUpdate, RouterError, RouterErrorKind, ToolExecution};

pub struct ActiveEntry {
    prepared: PreparedToolCall,
    execution: Box<dyn ToolExecution>,
    started_at: AuthorityInstant,
    next_sequence: u32,
}

impl ActiveEntry {
    pub(crate) const fn new(
        prepared: PreparedToolCall,
        execution: Box<dyn ToolExecution>,
        started_at: AuthorityInstant,
    ) -> Self {
        Self { prepared, execution, started_at, next_sequence: 0 }
    }

    pub(crate) const fn prepared(&self) -> &PreparedToolCall {
        &self.prepared
    }
    pub(crate) const fn started_at(&self) -> AuthorityInstant {
        self.started_at
    }
    pub(crate) const fn progress_count(&self) -> u32 {
        self.next_sequence
    }

    pub(crate) fn poll(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if deadline_reached(&self.prepared, observed_at) {
            self.execution.cancel(CancellationReason::Deadline, observed_at)
        } else {
            self.execution.poll(observed_at)
        }
    }

    pub(crate) fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        match control {
            ToolControl::Cancel(reason) => self.execution.cancel(reason, observed_at),
            other => self.execution.control(other, observed_at),
        }
    }

    pub(crate) fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.execution.cancel(reason, observed_at)
    }

    pub(crate) fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<crate::RecoveryObservation, DispatchFailure> {
        self.execution.recover(observed_at)
    }

    pub(crate) fn accept_update(&mut self, update: &ExecutionUpdate) -> Result<(), RouterError> {
        let progress = update.progress();
        if progress.first().is_some_and(|event| event.sequence() != self.next_sequence)
            || progress
                .windows(2)
                .any(|pair| pair[0].sequence().checked_add(1) != Some(pair[1].sequence()))
        {
            return Err(invalid("execution progress is not globally contiguous"));
        }
        let observed = u32::try_from(progress.len())
            .map_err(|_| invalid("execution progress count exceeds its integer bound"))?;
        self.next_sequence = self
            .next_sequence
            .checked_add(observed)
            .ok_or_else(|| invalid("execution progress sequence overflowed"))?;
        if let Some(result) = update.terminal() {
            validate_result(&self.prepared, result, self.next_sequence)?;
        }
        Ok(())
    }
}

pub fn ensure_supported(
    descriptor: &ToolDescriptor,
    control: &ToolControl,
) -> Result<(), RouterError> {
    let controls = descriptor.controls();
    let supported = match control {
        ToolControl::Poll => controls.poll(),
        ToolControl::Stdin(bytes) => {
            controls.stdin() && bytes.len() <= descriptor.limits().control_bytes() as usize
        }
        ToolControl::Resize { .. } => controls.resize(),
        ToolControl::Signal(signal) => {
            controls.signal()
                && signal.as_str().len() <= descriptor.limits().control_bytes() as usize
        }
        ToolControl::Cancel(_) => controls.cancel(),
    };
    if supported {
        Ok(())
    } else {
        Err(RouterError::new(
            RouterErrorKind::Control,
            "control tool execution",
            "descriptor does not advertise the control or its payload exceeds the bound",
        ))
    }
}

pub fn validate_result(
    prepared: &PreparedToolCall,
    result: &ToolResult,
    progress_count: u32,
) -> Result<(), RouterError> {
    if result.action_id() != prepared.call().action_id()
        || result.descriptor_digest() != prepared.descriptor_digest()
        || result.prepared_digest() != prepared.prepared_digest()
        || result.replay_identity() != prepared.replay_identity()
        || result.progress_count() != progress_count
        || (result.status() == ResultStatus::Succeeded) != result.failure_value().is_none()
    {
        return Err(invalid("terminal result differs from prepared call or closed status truth"));
    }
    Ok(())
}

fn deadline_reached(prepared: &PreparedToolCall, observed_at: AuthorityInstant) -> bool {
    observed_at.epoch() != prepared.call().deadline().epoch()
        || observed_at.tick_millis() >= prepared.call().deadline().tick_millis()
}

const fn invalid(detail: &'static str) -> RouterError {
    RouterError::new(RouterErrorKind::InvalidObservation, "accept tool observation", detail)
}
