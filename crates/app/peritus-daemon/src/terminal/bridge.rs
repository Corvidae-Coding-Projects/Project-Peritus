//! One live C2 process control and its bounded A3 attachment projections.

use std::collections::{BTreeMap, VecDeque};

use peritus_app_protocol::{
    TerminalAttachmentId, TerminalBinding, TerminalCancellation, TerminalDetach, TerminalInput,
    TerminalPhase, TerminalResize, TerminalStream, TerminalTransitionDisposition,
};
use peritus_process::{
    CancellationReason, ExecutionPlan, IoMode, ProcessControl, ProcessCursor, ProcessTreeIdentity,
    TerminalResult, TerminalSize,
};
use peritus_types::{ActorId, ProcessId, SessionId, Sha256Digest};

use super::{
    attachment::AttachmentRecord,
    error::{TerminalBridgeError, TerminalBridgeErrorKind},
    limits::TerminalRegistryLimits,
    registry::{AttachmentDisposition, TerminalBridgeEvent},
};

mod observation;

/// Complete checked registration supplied by the owner of a newly launched C2 process.
pub(crate) struct LiveTerminalRegistration {
    actor_id: ActorId,
    session_id: SessionId,
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    birth_identity: ProcessTreeIdentity,
    control: ProcessControl,
}

impl LiveTerminalRegistration {
    /// Builds a registration from the exact checked plan and observed native birth identity.
    ///
    /// # Errors
    ///
    /// Rejects pipe-mode plans and birth identities that cannot guard against PID reuse.
    pub(crate) fn new(
        plan: &ExecutionPlan,
        birth_identity: ProcessTreeIdentity,
        control: ProcessControl,
    ) -> Result<Self, TerminalBridgeError> {
        if !matches!(plan.io_mode(), IoMode::Pty(_)) {
            return Err(rejected(
                TerminalBridgeErrorKind::NotPty,
                "only a checked pseudo-terminal execution may be registered",
            ));
        }
        let capabilities = plan.terminal_capabilities();
        if capabilities.event_count() == 0 || capabilities.output_bytes() == 0 {
            return Err(rejected(
                TerminalBridgeErrorKind::NotPty,
                "terminal observation bounds are not authorized by the checked plan",
            ));
        }
        if birth_identity.root_pid() == 0
            || birth_identity.start_token().is_none()
            || !birth_identity.complete_containment()
        {
            return Err(rejected(
                TerminalBridgeErrorKind::BirthIdentityUnavailable,
                "an exact contained process birth identity is required for live attachment",
            ));
        }
        let identity = plan.identity();
        Ok(Self {
            actor_id: identity.actor_id(),
            session_id: identity.session_id(),
            process_id: identity.process_id(),
            plan_digest: plan.digest(),
            birth_identity,
            control,
        })
    }

    /// Returns the registered process identity.
    #[must_use]
    pub(crate) const fn process_id(&self) -> ProcessId {
        self.process_id
    }
}

#[derive(Clone)]
pub(super) struct ObservedOutput {
    pub(super) offset: u64,
    pub(super) stream: TerminalStream,
    pub(super) bytes: Vec<u8>,
}

pub(super) struct TerminalBridge {
    actor_id: ActorId,
    session_id: SessionId,
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    birth_identity: ProcessTreeIdentity,
    control: ProcessControl,
    process_cursor: ProcessCursor,
    stream_offsets: [u64; 3],
    next_output_offset: u64,
    replay: VecDeque<ObservedOutput>,
    replay_bytes: usize,
    replay_complete: bool,
    attachments: BTreeMap<TerminalAttachmentId, AttachmentRecord>,
    terminal: Option<TerminalResult>,
    fault: Option<(TerminalBridgeErrorKind, &'static str)>,
}

impl TerminalBridge {
    pub(super) fn new(registration: LiveTerminalRegistration) -> Self {
        Self {
            actor_id: registration.actor_id,
            session_id: registration.session_id,
            process_id: registration.process_id,
            plan_digest: registration.plan_digest,
            birth_identity: registration.birth_identity,
            control: registration.control,
            process_cursor: ProcessCursor::after(0),
            stream_offsets: [0; 3],
            next_output_offset: 0,
            replay: VecDeque::new(),
            replay_bytes: 0,
            replay_complete: true,
            attachments: BTreeMap::new(),
            terminal: None,
            fault: None,
        }
    }

    pub(super) fn owner_matches(&self, actor: ActorId, session: SessionId) -> bool {
        self.actor_id == actor && self.session_id == session
    }

    pub(super) fn attachment_count(&self) -> usize {
        self.attachments.len()
    }

    pub(super) fn attachment_matches(&self, binding: TerminalBinding) -> bool {
        self.attachments
            .get(&binding.attachment_id())
            .is_some_and(|record| record.binding() == binding)
    }

    pub(super) fn attach(
        &mut self,
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
        limits: TerminalRegistryLimits,
    ) -> Result<AttachmentDisposition, TerminalBridgeError> {
        self.require_binding_process(binding)?;
        self.observe(limits)?;
        if self.terminal.is_some() {
            return Err(rejected(
                TerminalBridgeErrorKind::ProcessNotLive,
                "terminal process already published its final result",
            ));
        }
        if let Some(existing) = self.attachments.get(&binding.attachment_id()) {
            return if existing.binding() == binding
                && existing.maximum_chunk_bytes() == maximum_chunk_bytes
                && existing.phase() == TerminalPhase::Attached
            {
                Ok(AttachmentDisposition::AlreadyAttached)
            } else {
                Err(rejected(
                    TerminalBridgeErrorKind::RegistrationConflict,
                    "terminal attachment identity is already bound to another fact",
                ))
            };
        }
        if !self.replay_complete {
            return Err(rejected(
                TerminalBridgeErrorKind::ReplayUnavailable,
                "the complete ordered output prefix is no longer retained",
            ));
        }
        let mut attachment = AttachmentRecord::new(binding, maximum_chunk_bytes)?;
        for output in &self.replay {
            attachment.enqueue_output(output, limits);
        }
        attachment.require_healthy()?;
        self.attachments.insert(binding.attachment_id(), attachment);
        Ok(AttachmentDisposition::Attached)
    }

    pub(super) fn input(&mut self, input: &TerminalInput) -> Result<(), TerminalBridgeError> {
        self.require_binding_process(input.binding())?;
        self.attachment_mut(input.binding())?.state().accept_input(input)?;
        self.control.write_stdin(input.bytes().to_vec())?;
        Ok(())
    }

    pub(super) fn resize(&mut self, resize: TerminalResize) -> Result<(), TerminalBridgeError> {
        self.require_binding_process(resize.binding())?;
        self.attachment_mut(resize.binding())?.state().resize(resize)?;
        let size = TerminalSize::new(resize.rows(), resize.columns(), 0, 0)?;
        self.control.resize(size)?;
        Ok(())
    }

    pub(super) fn detach(
        &mut self,
        detach: TerminalDetach,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        self.require_binding_process(detach.binding())?;
        let attachment = self.attachment_mut(detach.binding())?;
        let disposition = attachment.state_mut().detach(detach)?;
        attachment.clear_pending();
        Ok(disposition)
    }

    pub(super) fn cancel(
        &mut self,
        cancellation: TerminalCancellation,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        self.require_binding_process(cancellation.binding())?;
        let (next, disposition) = {
            let attachment = self.attachment_mut(cancellation.binding())?;
            let mut next = attachment.state().clone();
            let disposition = next.cancel(cancellation)?;
            (next, disposition)
        };
        if disposition == TerminalTransitionDisposition::Applied {
            self.control.cancel(CancellationReason::User)?;
        }
        let attachment = self.attachment_mut(cancellation.binding())?;
        *attachment.state_mut() = next;
        attachment.clear_pending();
        Ok(disposition)
    }

    pub(super) fn poll(
        &mut self,
        binding: TerminalBinding,
        limits: TerminalRegistryLimits,
    ) -> Result<Vec<TerminalBridgeEvent>, TerminalBridgeError> {
        self.require_binding_process(binding)?;
        self.observe(limits)?;
        self.attachment_mut(binding)?.drain(limits.maximum_delivery_events_per_poll(), limits)
    }

    pub(super) fn remove_attachment(&mut self, attachment_id: TerminalAttachmentId) {
        self.attachments.remove(&attachment_id);
    }

    pub(super) fn attachment_ids(&self) -> Vec<TerminalAttachmentId> {
        self.attachments.keys().copied().collect()
    }

    pub(super) fn can_retire(&self) -> bool {
        self.terminal.is_some() && self.attachments.values().all(AttachmentRecord::is_settled)
    }

    fn attachment_mut(
        &mut self,
        binding: TerminalBinding,
    ) -> Result<&mut AttachmentRecord, TerminalBridgeError> {
        self.attachments
            .get_mut(&binding.attachment_id())
            .filter(|record| record.binding() == binding)
            .ok_or_else(|| {
                rejected(
                    TerminalBridgeErrorKind::RegistrationConflict,
                    "terminal attachment binding is not registered exactly",
                )
            })
    }

    fn require_binding_process(&self, binding: TerminalBinding) -> Result<(), TerminalBridgeError> {
        if binding.process_id() == self.process_id {
            Ok(())
        } else {
            Err(rejected(
                TerminalBridgeErrorKind::ProcessIdentityMismatch,
                "terminal binding names another process",
            ))
        }
    }

    fn fail(&mut self, kind: TerminalBridgeErrorKind, detail: &'static str) {
        self.fault = Some((kind, detail));
        for attachment in self.attachments.values_mut() {
            attachment.mark_fault(kind, detail);
        }
    }
}

const fn rejected(kind: TerminalBridgeErrorKind, detail: &'static str) -> TerminalBridgeError {
    TerminalBridgeError::rejected(kind, detail)
}
