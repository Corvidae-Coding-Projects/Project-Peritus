//! One attachment's A3 state, bounded pending delivery, and output fencing.

use std::{collections::VecDeque, num::NonZeroUsize};

use peritus_app_protocol::{
    TerminalBinding, TerminalExit, TerminalExitDisposition, TerminalOutput, TerminalOutputGap,
    TerminalPhase, TerminalState,
};

use super::{
    bridge::ObservedOutput,
    error::{TerminalBridgeError, TerminalBridgeErrorKind},
    limits::TerminalRegistryLimits,
    registry::TerminalBridgeEvent,
};

#[derive(Clone, Copy)]
pub(super) struct AttachmentFault {
    kind: TerminalBridgeErrorKind,
    detail: &'static str,
}

impl AttachmentFault {
    pub(super) const fn into_error(self) -> TerminalBridgeError {
        TerminalBridgeError::rejected(self.kind, self.detail)
    }
}

pub(super) struct AttachmentRecord {
    state: TerminalState,
    delivered: TerminalState,
    maximum_chunk_bytes: NonZeroUsize,
    pending: VecDeque<TerminalBridgeEvent>,
    pending_bytes: usize,
    deferred_exit: Option<TerminalExit>,
    fault: Option<AttachmentFault>,
}

impl AttachmentRecord {
    pub(super) fn new(
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalBridgeError> {
        let maximum_chunk_bytes = NonZeroUsize::new(maximum_chunk_bytes).ok_or_else(|| {
            TerminalBridgeError::rejected(
                TerminalBridgeErrorKind::InvalidLimit,
                "terminal attachment chunk limit is zero",
            )
        })?;
        let state = TerminalState::new(binding, maximum_chunk_bytes.get())?;
        Ok(Self {
            delivered: state.clone(),
            state,
            maximum_chunk_bytes,
            pending: VecDeque::new(),
            pending_bytes: 0,
            deferred_exit: None,
            fault: None,
        })
    }

    pub(super) const fn binding(&self) -> TerminalBinding {
        self.state.binding()
    }

    pub(super) const fn phase(&self) -> TerminalPhase {
        self.state.phase()
    }

    pub(super) const fn maximum_chunk_bytes(&self) -> usize {
        self.maximum_chunk_bytes.get()
    }

    pub(super) const fn state(&self) -> &TerminalState {
        &self.state
    }

    pub(super) const fn state_mut(&mut self) -> &mut TerminalState {
        &mut self.state
    }

    pub(super) fn enqueue_output(
        &mut self,
        observed: &ObservedOutput,
        limits: TerminalRegistryLimits,
    ) {
        if self.fault.is_some() || self.state.phase() != TerminalPhase::Attached {
            return;
        }
        match self.try_enqueue_output(observed, limits) {
            Ok(()) => {}
            Err(error) if error.kind() == TerminalBridgeErrorKind::Backpressure => {
                if let Err(error) =
                    self.compact_to_gap(observed.next_offset, observed.stream_offsets_after)
                {
                    self.record_fault(error);
                }
            }
            Err(error) => self.record_fault(error),
        }
    }

    fn try_enqueue_output(
        &mut self,
        observed: &ObservedOutput,
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if observed.offset != self.state.next_output_offset()
            || observed.stream_offsets_before != self.state.stream_offsets()
        {
            return Err(TerminalBridgeError::rejected(
                TerminalBridgeErrorKind::ReplayUnavailable,
                "attachment output does not continue at its exact retained frontier",
            ));
        }
        let chunk_bound = self.maximum_chunk_bytes.get();
        let chunks = observed.bytes.len().div_ceil(chunk_bound);
        let next_events = self.pending.len().checked_add(chunks).ok_or_else(backpressure)?;
        let next_bytes =
            self.pending_bytes.checked_add(observed.bytes.len()).ok_or_else(backpressure)?;
        if next_events > limits.maximum_pending_events_per_attachment()
            || next_bytes > limits.maximum_pending_bytes_per_attachment()
        {
            return Err(backpressure());
        }

        let mut next_state = self.state.clone();
        let mut outputs = Vec::with_capacity(chunks);
        for bytes in observed.bytes.chunks(chunk_bound) {
            let output = TerminalOutput::new(
                next_state.binding(),
                next_state.next_output_sequence(),
                next_state.next_output_offset(),
                observed.stream,
                bytes.to_vec(),
                chunk_bound,
            )?;
            next_state.accept_output(&output)?;
            outputs.push(TerminalBridgeEvent::Output(output));
        }
        if next_state.next_output_offset() != observed.next_offset
            || next_state.stream_offsets() != observed.stream_offsets_after
        {
            return Err(TerminalBridgeError::rejected(
                TerminalBridgeErrorKind::ReplayUnavailable,
                "projected output does not reach its exact retained frontier",
            ));
        }
        self.state = next_state;
        self.pending.extend(outputs);
        self.pending_bytes = next_bytes;
        Ok(())
    }

    pub(super) fn enqueue_gap(
        &mut self,
        resume_offset: u64,
        stream_offsets: [u64; 3],
        limits: TerminalRegistryLimits,
    ) {
        if self.fault.is_some() || self.state.phase() != TerminalPhase::Attached {
            return;
        }
        if let Err(error) = self.try_enqueue_gap(resume_offset, stream_offsets, limits) {
            self.record_fault(error);
        }
    }

    fn try_enqueue_gap(
        &mut self,
        resume_offset: u64,
        stream_offsets: [u64; 3],
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if resume_offset == self.state.next_output_offset()
            && stream_offsets == self.state.stream_offsets()
        {
            return Ok(());
        }
        if self.pending.len() >= limits.maximum_pending_events_per_attachment() {
            return self.compact_to_gap(resume_offset, stream_offsets);
        }
        let mut next_state = self.state.clone();
        let gap = output_gap(&next_state, resume_offset, stream_offsets)?;
        next_state.accept_output_gap(gap)?;
        self.state = next_state;
        self.pending.push_back(TerminalBridgeEvent::Gap(gap));
        Ok(())
    }

    fn compact_to_gap(
        &mut self,
        resume_offset: u64,
        stream_offsets: [u64; 3],
    ) -> Result<(), TerminalBridgeError> {
        let mut next_state = self.delivered.clone();
        let gap = output_gap(&next_state, resume_offset, stream_offsets)?;
        next_state.accept_output_gap(gap)?;
        self.pending.clear();
        self.pending_bytes = 0;
        self.deferred_exit = None;
        self.state = next_state;
        self.pending.push_back(TerminalBridgeEvent::Gap(gap));
        Ok(())
    }

    pub(super) fn enqueue_exit(
        &mut self,
        disposition: TerminalExitDisposition,
        limits: TerminalRegistryLimits,
    ) -> Result<(), TerminalBridgeError> {
        if self.fault.is_some() || self.state.phase() != TerminalPhase::Attached {
            return Ok(());
        }
        let exit = TerminalExit::new(
            self.state.binding(),
            self.state.next_output_sequence(),
            self.state.next_output_offset(),
            disposition,
        );
        self.state.exit(exit)?;
        if self.pending.len() < limits.maximum_pending_events_per_attachment() {
            self.pending.push_back(TerminalBridgeEvent::Exited(exit));
        } else {
            self.deferred_exit = Some(exit);
        }
        Ok(())
    }

    pub(super) fn drain(
        &mut self,
        maximum: usize,
        limits: TerminalRegistryLimits,
    ) -> Result<Vec<TerminalBridgeEvent>, TerminalBridgeError> {
        if self.pending.is_empty() {
            if let Some(fault) = self.fault {
                return Err(fault.into_error());
            }
            self.promote_exit(limits);
        }
        let count = maximum.min(self.pending.len());
        let mut events = Vec::with_capacity(count);
        for _ in 0..count {
            let Some(event) = self.pending.front().cloned() else { break };
            self.accept_delivered(&event)?;
            let _ = self.pending.pop_front();
            if let TerminalBridgeEvent::Output(output) = &event {
                self.pending_bytes = self.pending_bytes.saturating_sub(output.bytes().len());
            }
            events.push(event);
        }
        self.promote_exit(limits);
        Ok(events)
    }

    fn accept_delivered(
        &mut self,
        event: &TerminalBridgeEvent,
    ) -> Result<(), TerminalBridgeError> {
        match event {
            TerminalBridgeEvent::Output(output) => self.delivered.accept_output(output)?,
            TerminalBridgeEvent::Gap(gap) => self.delivered.accept_output_gap(*gap)?,
            TerminalBridgeEvent::Exited(exit) => self.delivered.exit(*exit)?,
        }
        Ok(())
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_bytes = 0;
        self.deferred_exit = None;
        self.delivered = self.state.clone();
    }

    pub(super) fn require_healthy(&self) -> Result<(), TerminalBridgeError> {
        self.fault.map_or(Ok(()), |fault| Err(fault.into_error()))
    }

    pub(super) fn is_settled(&self) -> bool {
        self.state.phase() != TerminalPhase::Attached
            && self.pending.is_empty()
            && self.deferred_exit.is_none()
    }

    pub(super) const fn mark_fault(&mut self, kind: TerminalBridgeErrorKind, detail: &'static str) {
        self.fault = Some(AttachmentFault { kind, detail });
    }

    fn record_fault(&mut self, error: TerminalBridgeError) {
        self.fault = Some(AttachmentFault {
            kind: error.kind(),
            detail: "terminal attachment output projection failed",
        });
    }

    fn promote_exit(&mut self, limits: TerminalRegistryLimits) {
        if self.fault.is_none()
            && self.pending.len() < limits.maximum_pending_events_per_attachment()
            && let Some(exit) = self.deferred_exit.take()
        {
            self.pending.push_back(TerminalBridgeEvent::Exited(exit));
        }
    }
}

fn output_gap(
    state: &TerminalState,
    resume_offset: u64,
    stream_offsets: [u64; 3],
) -> Result<TerminalOutputGap, TerminalBridgeError> {
    TerminalOutputGap::new(
        state.binding(),
        state.next_output_sequence(),
        state.next_output_offset(),
        resume_offset,
        stream_offsets[0],
        stream_offsets[1],
        stream_offsets[2],
    )
    .map_err(Into::into)
}

const fn backpressure() -> TerminalBridgeError {
    TerminalBridgeError::rejected(
        TerminalBridgeErrorKind::Backpressure,
        "terminal attachment pending output exceeded its bound",
    )
}
