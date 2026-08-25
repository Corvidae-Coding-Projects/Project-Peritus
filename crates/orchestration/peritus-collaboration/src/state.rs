//! Complete authoritative D3 collaboration state.

use peritus_types::{CommandId, EventId, EventSequence, RunId, Sha256Digest};

use crate::{
    CollaborationBinding, CollaborationLimits, CollaborationMessageId, CollaborationTask,
    CollaborationTaskId, JoinPolicy, MessageDelivery, TaskPhase, TaskTerminalKind,
};

pub mod mutation;
mod validation;

/// Closed aggregate lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CollaborationPhase {
    /// Delegation and all active commands are admitted.
    Active,
    /// New delegation is paused while ownership and delivery remain explicit.
    Paused,
    /// A truthful immutable aggregate terminal was committed.
    Terminal,
}

/// Closed aggregate terminal kinds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CollaborationTerminalKind {
    /// Root and all required joined descendants succeeded with no pending work.
    Completed,
    /// One or more required tasks failed or were rejected.
    Failed,
    /// Root or a required task was cancelled.
    Cancelled,
    /// Root or a required task was abandoned.
    Abandoned,
    /// Required join membership remained unsatisfied.
    UnsatisfiedJoin,
}

/// Truthful aggregate terminal with canonical blocking task identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationTerminal {
    kind: CollaborationTerminalKind,
    blocking_tasks: Vec<CollaborationTaskId>,
    digest: Sha256Digest,
}

impl CollaborationTerminal {
    pub(super) fn new(
        kind: CollaborationTerminalKind,
        blocking_tasks: Vec<CollaborationTaskId>,
    ) -> Self {
        let mut terminal = Self { kind, blocking_tasks, digest: Sha256Digest::new([0; 32]) };
        terminal.digest = crate::canonical::terminal_digest(&terminal);
        terminal
    }
    pub(super) const fn from_wire(
        kind: CollaborationTerminalKind,
        blocking_tasks: Vec<CollaborationTaskId>,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, blocking_tasks, digest }
    }
    /// Returns the truthful aggregate outcome.
    #[must_use]
    pub const fn kind(&self) -> CollaborationTerminalKind {
        self.kind
    }
    /// Borrows canonical task identities explaining non-success.
    #[must_use]
    pub const fn blocking_tasks(&self) -> &[CollaborationTaskId] {
        self.blocking_tasks.as_slice()
    }
    /// Returns canonical terminal digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Complete deterministic replayable collaboration aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationState {
    binding: CollaborationBinding,
    phase: CollaborationPhase,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    tasks: Vec<CollaborationTask>,
    messages: Vec<MessageDelivery>,
    used_commands: Vec<CommandId>,
    terminal: Option<CollaborationTerminal>,
}

impl CollaborationState {
    pub(super) fn genesis(
        binding: CollaborationBinding,
        sequence: EventSequence,
        event_id: EventId,
        command_id: CommandId,
    ) -> Self {
        let root = CollaborationTask::accepted_root(binding.root_assignment().clone());
        Self {
            binding,
            phase: CollaborationPhase::Active,
            sequence,
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            tasks: vec![root],
            messages: Vec::new(),
            used_commands: vec![command_id],
            terminal: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        binding: CollaborationBinding,
        phase: CollaborationPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        tasks: Vec<CollaborationTask>,
        messages: Vec<MessageDelivery>,
        used_commands: Vec<CommandId>,
        terminal: Option<CollaborationTerminal>,
    ) -> Self {
        Self {
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            tasks,
            messages,
            used_commands,
            terminal,
        }
    }

    /// Borrows the immutable aggregate binding.
    #[must_use]
    pub const fn binding(&self) -> &CollaborationBinding {
        &self.binding
    }
    /// Returns run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.binding.run_id()
    }
    /// Returns immutable bounds.
    #[must_use]
    pub const fn limits(&self) -> CollaborationLimits {
        self.binding.limits()
    }
    /// Returns aggregate phase.
    #[must_use]
    pub const fn phase(&self) -> CollaborationPhase {
        self.phase
    }
    /// Returns latest one-based sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns latest event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns complete canonical state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Borrows canonical task records sorted by task identity.
    #[must_use]
    pub const fn tasks(&self) -> &[CollaborationTask] {
        self.tasks.as_slice()
    }
    /// Borrows message deliveries in canonical message identity order.
    #[must_use]
    pub const fn messages(&self) -> &[MessageDelivery] {
        self.messages.as_slice()
    }
    /// Borrows consumed command identities in event order.
    #[must_use]
    pub const fn used_commands(&self) -> &[CommandId] {
        self.used_commands.as_slice()
    }
    /// Borrows the truthful aggregate terminal.
    #[must_use]
    pub const fn terminal(&self) -> Option<&CollaborationTerminal> {
        self.terminal.as_ref()
    }

    /// Looks up one task by stable identity.
    #[must_use]
    pub fn task(&self, task_id: CollaborationTaskId) -> Option<&CollaborationTask> {
        self.tasks
            .binary_search_by_key(&task_id, |task| task.assignment().task_id())
            .ok()
            .map(|index| &self.tasks[index])
    }

    /// Looks up one message delivery by stable identity.
    #[must_use]
    pub fn message(&self, message_id: CollaborationMessageId) -> Option<&MessageDelivery> {
        self.messages
            .binary_search_by_key(&message_id, |delivery| delivery.message().id())
            .ok()
            .map(|index| &self.messages[index])
    }

    /// Returns direct children in canonical task identity order.
    #[must_use]
    pub fn children(&self, parent: CollaborationTaskId) -> Vec<&CollaborationTask> {
        self.tasks
            .iter()
            .filter(|task| task.assignment().parent_task_id() == Some(parent))
            .collect()
    }

    /// Returns whether a candidate is in the target's descendant subtree.
    #[must_use]
    pub fn is_descendant_of(
        &self,
        candidate: CollaborationTaskId,
        target: CollaborationTaskId,
    ) -> bool {
        let mut cursor = Some(candidate);
        for _ in 0..=self.limits().depth() {
            let Some(current) = cursor else {
                return false;
            };
            if current == target {
                return candidate != target;
            }
            cursor = self.task(current).and_then(|task| task.assignment().parent_task_id());
        }
        false
    }

    /// Returns whether all declared child joins permit this task to claim success.
    #[must_use]
    pub fn join_satisfied(&self, task_id: CollaborationTaskId) -> bool {
        let Some(task) = self.task(task_id) else {
            return false;
        };
        let required: Vec<_> = self
            .children(task_id)
            .into_iter()
            .filter(|child| child.assignment().required())
            .collect();
        match task.assignment().join_policy() {
            JoinPolicy::NoChildren => required.is_empty(),
            JoinPolicy::AllRequired => required.iter().all(|child| {
                child
                    .terminal()
                    .is_some_and(|terminal| terminal.kind() == TaskTerminalKind::Succeeded)
            }),
            JoinPolicy::AnyRequired => {
                !required.is_empty()
                    && required.iter().any(|child| {
                        child
                            .terminal()
                            .is_some_and(|terminal| terminal.kind() == TaskTerminalKind::Succeeded)
                    })
            }
        }
    }

    /// Returns whether any delivery or cancellation acknowledgement remains pending.
    #[must_use]
    pub fn has_pending_directives(&self) -> bool {
        self.messages.iter().any(|delivery| !delivery.acknowledged())
            || self.tasks.iter().any(|task| task.phase() == TaskPhase::Cancelling)
    }

    /// Returns a conservative deterministic encoded-size upper estimate.
    #[must_use]
    pub fn estimated_encoded_bytes(&self) -> u64 {
        let task_bytes = (self.tasks.len() as u64).saturating_mul(512);
        let message_bytes = self.messages.iter().fold(0_u64, |total, delivery| {
            total
                .saturating_add(u64::from(delivery.message().payload_bytes()))
                .saturating_add(delivery.message().media_type().len() as u64)
                .saturating_add(384)
        });
        task_bytes
            .saturating_add(message_bytes)
            .saturating_add((self.used_commands.len() as u64).saturating_mul(16))
            .saturating_add(1_024)
    }

    pub(super) fn validate_inert(&self) -> Result<(), crate::CollaborationError> {
        validation::validate(self)
    }
}
