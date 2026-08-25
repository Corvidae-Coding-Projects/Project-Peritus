//! Task assignment, ownership, reservation, cancellation, and terminal records.

use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, WorkId};
use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::{ArtifactHandoff, CollaborationTaskId, JoinPolicy};

/// Immutable assignment and causal placement of one collaboration task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delegation {
    task_id: CollaborationTaskId,
    root_task_id: CollaborationTaskId,
    parent_task_id: Option<CollaborationTaskId>,
    depth: u16,
    owner: ActorId,
    role: HarnessRole,
    parent_owner: ActorId,
    work_id: WorkId,
    goal_digest: Sha256Digest,
    required: bool,
    join_policy: JoinPolicy,
}

impl Delegation {
    /// Creates the root assignment at depth zero.
    ///
    /// # Errors
    /// Rejects an all-zero goal digest.
    pub fn root(
        task_id: CollaborationTaskId,
        owner: ActorId,
        role: HarnessRole,
        work_id: WorkId,
        goal_digest: Sha256Digest,
        join_policy: JoinPolicy,
    ) -> Result<Self, CollaborationError> {
        Self::new(
            task_id,
            task_id,
            None,
            0,
            owner,
            role,
            owner,
            work_id,
            goal_digest,
            true,
            join_policy,
        )
    }

    /// Creates a child assignment with explicit immutable causality.
    ///
    /// # Errors
    /// Rejects self-parenting, a zero depth, or an all-zero goal digest.
    #[allow(clippy::too_many_arguments, reason = "causal and ownership bindings stay explicit")]
    pub fn child(
        task_id: CollaborationTaskId,
        root_task_id: CollaborationTaskId,
        parent_task_id: CollaborationTaskId,
        depth: u16,
        owner: ActorId,
        role: HarnessRole,
        parent_owner: ActorId,
        work_id: WorkId,
        goal_digest: Sha256Digest,
        required: bool,
        join_policy: JoinPolicy,
    ) -> Result<Self, CollaborationError> {
        Self::new(
            task_id,
            root_task_id,
            Some(parent_task_id),
            depth,
            owner,
            role,
            parent_owner,
            work_id,
            goal_digest,
            required,
            join_policy,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        task_id: CollaborationTaskId,
        root_task_id: CollaborationTaskId,
        parent_task_id: Option<CollaborationTaskId>,
        depth: u16,
        owner: ActorId,
        role: HarnessRole,
        parent_owner: ActorId,
        work_id: WorkId,
        goal_digest: Sha256Digest,
        required: bool,
        join_policy: JoinPolicy,
    ) -> Result<Self, CollaborationError> {
        if goal_digest == Sha256Digest::new([0; 32]) {
            return Err(reject(
                CollaborationErrorKind::InvalidInput,
                "task goal digest must be nonzero",
            ));
        }
        match parent_task_id {
            None if depth != 0 || task_id != root_task_id || owner != parent_owner => {
                return Err(reject(
                    CollaborationErrorKind::CausalityViolation,
                    "root assignment has inconsistent root, depth, or parent owner",
                ));
            }
            Some(parent) if depth == 0 || parent == task_id => {
                return Err(reject(
                    CollaborationErrorKind::CausalityViolation,
                    "child assignment has zero depth or self-parenting",
                ));
            }
            _ => {}
        }
        Ok(Self {
            task_id,
            root_task_id,
            parent_task_id,
            depth,
            owner,
            role,
            parent_owner,
            work_id,
            goal_digest,
            required,
            join_policy,
        })
    }

    /// Returns the stable task identity.
    #[must_use]
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns the stable root identity.
    #[must_use]
    pub const fn root_task_id(&self) -> CollaborationTaskId {
        self.root_task_id
    }
    /// Returns the direct causal parent.
    #[must_use]
    pub const fn parent_task_id(&self) -> Option<CollaborationTaskId> {
        self.parent_task_id
    }
    /// Returns immutable causal depth.
    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }
    /// Returns the assigned owner.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns the observed harness role.
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    /// Returns the owner that offered the assignment.
    #[must_use]
    pub const fn parent_owner(&self) -> ActorId {
        self.parent_owner
    }
    /// Returns the scheduler work binding.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Returns the inert goal digest.
    #[must_use]
    pub const fn goal_digest(&self) -> Sha256Digest {
        self.goal_digest
    }
    /// Returns whether the parent declared this child required.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }
    /// Returns this task's child-join policy.
    #[must_use]
    pub const fn join_policy(&self) -> JoinPolicy {
        self.join_policy
    }
}

/// Narrow scheduler reservation observation required before task activation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationObservation {
    work_id: WorkId,
    dispatch_id: DispatchId,
    owner: ActorId,
    revision: RevisionTuple,
}

impl ReservationObservation {
    /// Creates an inert exact scheduler reservation observation.
    #[must_use]
    pub const fn new(
        work_id: WorkId,
        dispatch_id: DispatchId,
        owner: ActorId,
        revision: RevisionTuple,
    ) -> Self {
        Self { work_id, dispatch_id, owner, revision }
    }
    /// Returns the scheduler work identity.
    #[must_use]
    pub const fn work_id(self) -> WorkId {
        self.work_id
    }
    /// Returns the committed dispatch identity.
    #[must_use]
    pub const fn dispatch_id(self) -> DispatchId {
        self.dispatch_id
    }
    /// Returns the scheduler-observed owner.
    #[must_use]
    pub const fn owner(self) -> ActorId {
        self.owner
    }
    /// Returns the exact revision fence.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
}

/// Closed task lifecycle including explicit delegation acceptance and cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskPhase {
    /// Parent owner durably offered the task.
    Offered,
    /// Assigned owner durably accepted the task.
    Accepted,
    /// Exact scheduler reservation was observed and work became active.
    Active,
    /// Cancellation is pending owner acknowledgement.
    Cancelling,
    /// A truthful immutable task outcome was retained.
    Terminal,
}

/// Complete retained collaboration task record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationTask {
    assignment: Delegation,
    phase: TaskPhase,
    reservation: Option<ReservationObservation>,
    terminal: Option<TaskTerminal>,
}

impl CollaborationTask {
    pub(super) const fn offered(assignment: Delegation) -> Self {
        Self { assignment, phase: TaskPhase::Offered, reservation: None, terminal: None }
    }

    pub(super) const fn accepted_root(assignment: Delegation) -> Self {
        Self { assignment, phase: TaskPhase::Accepted, reservation: None, terminal: None }
    }

    pub(super) const fn from_wire(
        assignment: Delegation,
        phase: TaskPhase,
        reservation: Option<ReservationObservation>,
        terminal: Option<TaskTerminal>,
    ) -> Self {
        Self { assignment, phase, reservation, terminal }
    }

    /// Borrows the immutable assignment.
    #[must_use]
    pub const fn assignment(&self) -> &Delegation {
        &self.assignment
    }
    /// Returns the current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> TaskPhase {
        self.phase
    }
    /// Returns the exact observed scheduler reservation.
    #[must_use]
    pub const fn reservation(&self) -> Option<ReservationObservation> {
        self.reservation
    }
    /// Returns the retained terminal outcome.
    #[must_use]
    pub const fn terminal(&self) -> Option<TaskTerminal> {
        self.terminal
    }

    pub(super) const fn set_phase(&mut self, phase: TaskPhase) {
        self.phase = phase;
    }
    pub(super) const fn set_reservation(&mut self, reservation: ReservationObservation) {
        self.reservation = Some(reservation);
    }
    pub(super) const fn terminate(&mut self, terminal: TaskTerminal) {
        self.phase = TaskPhase::Terminal;
        self.terminal = Some(terminal);
    }
}

/// Closed truthful task terminal kinds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskTerminalKind {
    /// Work succeeded and all required joins were satisfied.
    Succeeded,
    /// Work failed explicitly.
    Failed,
    /// An offered assignment was rejected.
    Rejected,
    /// Cancellation was acknowledged or applied before activation.
    Cancelled,
    /// Ownership ended without a completion claim.
    Abandoned,
}

/// Immutable terminal outcome with optional exact artifact handoff.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskTerminal {
    kind: TaskTerminalKind,
    handoff: Option<ArtifactHandoff>,
    cause_digest: Sha256Digest,
}

impl TaskTerminal {
    /// Creates a checked terminal outcome.
    ///
    /// # Errors
    /// Rejects a handoff on non-success or a zero cause digest for failure/abandonment.
    pub fn new(
        kind: TaskTerminalKind,
        handoff: Option<ArtifactHandoff>,
        cause_digest: Sha256Digest,
    ) -> Result<Self, CollaborationError> {
        if kind != TaskTerminalKind::Succeeded && handoff.is_some() {
            return Err(reject(
                CollaborationErrorKind::InvalidInput,
                "only a successful task may retain an artifact handoff",
            ));
        }
        if matches!(kind, TaskTerminalKind::Failed | TaskTerminalKind::Abandoned)
            && cause_digest == Sha256Digest::new([0; 32])
        {
            return Err(reject(
                CollaborationErrorKind::InvalidInput,
                "failure and abandonment require a nonzero cause digest",
            ));
        }
        Ok(Self { kind, handoff, cause_digest })
    }

    /// Returns the truthful outcome.
    #[must_use]
    pub const fn kind(self) -> TaskTerminalKind {
        self.kind
    }
    /// Returns the exact artifact/evidence handoff when present.
    #[must_use]
    pub const fn handoff(self) -> Option<ArtifactHandoff> {
        self.handoff
    }
    /// Returns the inert cause digest.
    #[must_use]
    pub const fn cause_digest(self) -> Sha256Digest {
        self.cause_digest
    }
}

/// One deterministic task phase change caused by cancellation propagation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CancellationEffect {
    task_id: CollaborationTaskId,
    resulting_phase: TaskPhase,
}

impl CancellationEffect {
    pub(super) const fn new(task_id: CollaborationTaskId, resulting_phase: TaskPhase) -> Self {
        Self { task_id, resulting_phase }
    }
    /// Returns the affected task.
    #[must_use]
    pub const fn task_id(self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns the exact successor phase.
    #[must_use]
    pub const fn resulting_phase(self) -> TaskPhase {
        self.resulting_phase
    }
}
