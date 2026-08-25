//! Rebuildable read-only projections over authoritative collaboration state.

use peritus_role::HarnessRole;
use peritus_scheduler::WorkId;
use peritus_types::{ActorId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    CollaborationMessageId, CollaborationPhase, CollaborationState, CollaborationTaskId,
    CollaborationTerminalKind, JoinPolicy, TaskPhase, TaskTerminalKind,
};

/// One retained task query row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedTask {
    task_id: CollaborationTaskId,
    parent: Option<CollaborationTaskId>,
    depth: u16,
    owner: ActorId,
    role: HarnessRole,
    work_id: WorkId,
    required: bool,
    join_policy: JoinPolicy,
    phase: TaskPhase,
    terminal: Option<TaskTerminalKind>,
}

impl ProjectedTask {
    /// Returns task identity.
    #[must_use]
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns direct causal parent.
    #[must_use]
    pub const fn parent(&self) -> Option<CollaborationTaskId> {
        self.parent
    }
    /// Returns immutable depth.
    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }
    /// Returns assigned owner.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns observed harness role.
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    /// Returns scheduler work binding.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Returns whether this is required by its parent.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }
    /// Returns declared child-join policy.
    #[must_use]
    pub const fn join_policy(&self) -> JoinPolicy {
        self.join_policy
    }
    /// Returns task lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> TaskPhase {
        self.phase
    }
    /// Returns terminal outcome when present.
    #[must_use]
    pub const fn terminal(&self) -> Option<TaskTerminalKind> {
        self.terminal
    }
}

/// One retained message-delivery query row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedMessage {
    message_id: CollaborationMessageId,
    task_id: CollaborationTaskId,
    ordinal: u32,
    sender: ActorId,
    receiver: ActorId,
    content_digest: Sha256Digest,
    acknowledged: bool,
}

impl ProjectedMessage {
    /// Returns message identity.
    #[must_use]
    pub const fn message_id(&self) -> CollaborationMessageId {
        self.message_id
    }
    /// Returns causal task.
    #[must_use]
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns contiguous task-local ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// Returns sender.
    #[must_use]
    pub const fn sender(&self) -> ActorId {
        self.sender
    }
    /// Returns receiver.
    #[must_use]
    pub const fn receiver(&self) -> ActorId {
        self.receiver
    }
    /// Returns inert content digest.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    /// Returns delivery acknowledgement state.
    #[must_use]
    pub const fn acknowledged(&self) -> bool {
        self.acknowledged
    }
}

/// Complete authority-free collaboration query projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationProjection {
    run_id: RunId,
    revision: RevisionTuple,
    phase: CollaborationPhase,
    terminal: Option<CollaborationTerminalKind>,
    sequence: u64,
    state_digest: Sha256Digest,
    pending_deliveries: usize,
    cancelling_tasks: usize,
    tasks: Vec<ProjectedTask>,
    messages: Vec<ProjectedMessage>,
}

impl CollaborationProjection {
    /// Projects one authoritative state deterministically.
    #[must_use]
    pub fn from_state(state: &CollaborationState) -> Self {
        let tasks = state
            .tasks()
            .iter()
            .map(|task| ProjectedTask {
                task_id: task.assignment().task_id(),
                parent: task.assignment().parent_task_id(),
                depth: task.assignment().depth(),
                owner: task.assignment().owner(),
                role: task.assignment().role(),
                work_id: task.assignment().work_id(),
                required: task.assignment().required(),
                join_policy: task.assignment().join_policy(),
                phase: task.phase(),
                terminal: task.terminal().map(crate::TaskTerminal::kind),
            })
            .collect();
        let messages = state
            .messages()
            .iter()
            .map(|delivery| ProjectedMessage {
                message_id: delivery.message().id(),
                task_id: delivery.message().task_id(),
                ordinal: delivery.message().ordinal(),
                sender: delivery.message().sender(),
                receiver: delivery.message().receiver(),
                content_digest: delivery.message().content_digest(),
                acknowledged: delivery.acknowledged(),
            })
            .collect();
        Self {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            phase: state.phase(),
            terminal: state.terminal().map(crate::CollaborationTerminal::kind),
            sequence: state.sequence().get(),
            state_digest: state.state_digest(),
            pending_deliveries: state.messages().iter().filter(|item| !item.acknowledged()).count(),
            cancelling_tasks: state
                .tasks()
                .iter()
                .filter(|task| task.phase() == TaskPhase::Cancelling)
                .count(),
            tasks,
            messages,
        }
    }
    /// Returns run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns aggregate phase.
    #[must_use]
    pub const fn phase(&self) -> CollaborationPhase {
        self.phase
    }
    /// Returns aggregate terminal kind.
    #[must_use]
    pub const fn terminal(&self) -> Option<CollaborationTerminalKind> {
        self.terminal
    }
    /// Returns latest sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns canonical state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns pending delivery count.
    #[must_use]
    pub const fn pending_deliveries(&self) -> usize {
        self.pending_deliveries
    }
    /// Returns cancelling task count.
    #[must_use]
    pub const fn cancelling_tasks(&self) -> usize {
        self.cancelling_tasks
    }
    /// Borrows task rows.
    #[must_use]
    pub fn tasks(&self) -> &[ProjectedTask] {
        &self.tasks
    }
    /// Borrows message rows.
    #[must_use]
    pub fn messages(&self) -> &[ProjectedMessage] {
        &self.messages
    }
}
