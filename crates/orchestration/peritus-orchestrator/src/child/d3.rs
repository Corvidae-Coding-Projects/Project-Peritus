//! D3 scheduler and collaboration terminal observations.

use peritus_collaboration::{
    CollaborationPhase, CollaborationState, CollaborationTaskId, CollaborationTerminalKind,
    TaskPhase,
};
use peritus_role::HarnessRole;
use peritus_scheduler::{
    DispatchId, SchedulerPhase, SchedulerState, SchedulerTerminalKind, WorkId, WorkPhase, WorkerId,
};
use peritus_types::{ActorId, RevisionTuple, RunId};

use super::{ChildAggregateKind, ChildHead, ChildTerminalClass, binding, stale};
use crate::{Handoff, HandoffId, OrchestratorError, QualityCycleBinding};

/// Checked proof that D3 reserved and activated one exact E0 handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffActivationObservation {
    handoff_id: HandoffId,
    task_id: CollaborationTaskId,
    work_id: WorkId,
    dispatch_id: DispatchId,
    worker_id: WorkerId,
    owner: ActorId,
    role: HarnessRole,
    scheduler_run_id: RunId,
    collaboration_run_id: RunId,
    revision: RevisionTuple,
    scheduler_head: ChildHead,
    collaboration_head: ChildHead,
}

impl HandoffActivationObservation {
    /// Checks both authoritative D3 states before E0 may consume a child result.
    ///
    /// # Errors
    ///
    /// Returns an error when either D3 state differs from the cycle or handoff.
    pub fn from_states(
        scheduler: &SchedulerState,
        collaboration: &CollaborationState,
        cycle: &QualityCycleBinding,
        handoff: &Handoff,
    ) -> Result<Self, OrchestratorError> {
        let role = handoff
            .destination_role()
            .harness_role()
            .ok_or_else(|| binding("D3 handoff destination has no harness role"))?;
        if scheduler.run_id() != cycle.scheduler_run_id()
            || collaboration.run_id() != cycle.collaboration_run_id()
            || scheduler.binding().scheduler_id() != cycle.scheduler_id()
            || scheduler.binding().digest() != cycle.scheduler_binding_digest()
            || collaboration.binding().id() != cycle.collaboration_id()
            || collaboration.binding().digest() != cycle.collaboration_binding_digest()
            || collaboration.binding().scheduler_id() != cycle.scheduler_id()
            || scheduler.binding().revision() != handoff.candidate().revision()
            || collaboration.binding().revision() != handoff.candidate().revision()
        {
            return Err(binding("D3 aggregates differ from the E0 handoff binding"));
        }
        let task = collaboration
            .task(handoff.task_id())
            .ok_or_else(|| binding("D3 collaboration task is absent"))?;
        let activation =
            task.reservation().ok_or_else(|| stale("D3 collaboration task has no reservation"))?;
        let reservation = scheduler
            .reservation(activation.dispatch_id())
            .ok_or_else(|| stale("D3 scheduler reservation is absent"))?;
        let work = scheduler
            .work_item(handoff.work_id())
            .ok_or_else(|| binding("D3 scheduler work is absent"))?;
        if task.phase() != TaskPhase::Active
            || task.assignment().task_id() != handoff.task_id()
            || task.assignment().work_id() != handoff.work_id()
            || task.assignment().owner() != handoff.destination_actor()
            || task.assignment().role() != role
            || activation.work_id() != handoff.work_id()
            || activation.owner() != handoff.destination_actor()
            || activation.revision() != handoff.candidate().revision()
            || reservation.work_id() != handoff.work_id()
            || reservation.owner() != handoff.destination_actor()
            || reservation.revision() != handoff.candidate().revision()
            || !reservation.started()
            || work.phase() != WorkPhase::Running
            || work.spec().owner() != handoff.destination_actor()
            || work.spec().revision() != handoff.candidate().revision()
        {
            return Err(binding("D3 task, work, reservation, owner, role, or revision differs"));
        }
        Ok(Self {
            handoff_id: handoff.id(),
            task_id: handoff.task_id(),
            work_id: handoff.work_id(),
            dispatch_id: reservation.dispatch_id(),
            worker_id: reservation.worker_id(),
            owner: handoff.destination_actor(),
            role,
            scheduler_run_id: scheduler.run_id(),
            collaboration_run_id: collaboration.run_id(),
            revision: handoff.candidate().revision(),
            scheduler_head: ChildHead::new(
                ChildAggregateKind::Scheduler,
                scheduler.sequence(),
                scheduler.last_event_id(),
                scheduler.state_digest(),
                None,
            )?,
            collaboration_head: ChildHead::new(
                ChildAggregateKind::Collaboration,
                collaboration.sequence(),
                collaboration.last_event_id(),
                collaboration.state_digest(),
                None,
            )?,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "D3 activation wire binding stays explicit")]
    pub(crate) fn from_wire(
        handoff_id: HandoffId,
        task_id: CollaborationTaskId,
        work_id: WorkId,
        dispatch_id: DispatchId,
        worker_id: WorkerId,
        owner: ActorId,
        role: HarnessRole,
        scheduler_run_id: RunId,
        collaboration_run_id: RunId,
        revision: RevisionTuple,
        scheduler_head: ChildHead,
        collaboration_head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        if scheduler_head.aggregate() != ChildAggregateKind::Scheduler
            || collaboration_head.aggregate() != ChildAggregateKind::Collaboration
            || scheduler_head.terminal().is_some()
            || collaboration_head.terminal().is_some()
        {
            return Err(binding("decoded D3 activation heads are inconsistent"));
        }
        Ok(Self {
            handoff_id,
            task_id,
            work_id,
            dispatch_id,
            worker_id,
            owner,
            role,
            scheduler_run_id,
            collaboration_run_id,
            revision,
            scheduler_head,
            collaboration_head,
        })
    }

    #[must_use]
    /// Returns the exact E0 handoff activated by D3.
    pub const fn handoff_id(&self) -> HandoffId {
        self.handoff_id
    }
    #[must_use]
    /// Returns the collaboration task activated for the handoff.
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    #[must_use]
    /// Returns the scheduler work item activated for the handoff.
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    #[must_use]
    /// Returns the scheduler dispatch reservation identity.
    pub const fn dispatch_id(&self) -> DispatchId {
        self.dispatch_id
    }
    #[must_use]
    /// Returns the worker bound by the scheduler reservation.
    pub const fn worker_id(&self) -> WorkerId {
        self.worker_id
    }
    #[must_use]
    /// Returns the actor owning the activated handoff.
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    #[must_use]
    /// Returns the harness role assigned to the owner.
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[must_use]
    /// Returns the exact scheduler child run identity.
    pub const fn scheduler_run_id(&self) -> RunId {
        self.scheduler_run_id
    }
    #[must_use]
    /// Returns the exact collaboration child run identity.
    pub const fn collaboration_run_id(&self) -> RunId {
        self.collaboration_run_id
    }
    #[must_use]
    /// Returns the candidate revision activated by D3.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the nonterminal scheduler head proving activation.
    pub const fn scheduler_head(&self) -> ChildHead {
        self.scheduler_head
    }
    #[must_use]
    /// Returns the nonterminal collaboration head proving activation.
    pub const fn collaboration_head(&self) -> ChildHead {
        self.collaboration_head
    }
}

/// Checked terminal D3 scheduler observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerChildObservation {
    run_id: RunId,
    revision: RevisionTuple,
    head: ChildHead,
}

impl SchedulerChildObservation {
    /// Projects one terminal scheduler state.
    ///
    /// # Errors
    ///
    /// Returns an error when the state is nonterminal or differs from the current cycle.
    pub fn from_state(
        state: &SchedulerState,
        cycle: &QualityCycleBinding,
    ) -> Result<Self, OrchestratorError> {
        if state.run_id() != cycle.scheduler_run_id()
            || state.binding().scheduler_id() != cycle.scheduler_id()
            || state.binding().digest() != cycle.scheduler_binding_digest()
            || state.binding().revision() != cycle.revision()
        {
            return Err(binding("terminal scheduler state differs from the current cycle"));
        }
        if state.phase() != SchedulerPhase::Terminal {
            return Err(stale("scheduler observation is not terminal"));
        }
        let terminal =
            state.terminal().ok_or_else(|| binding("terminal scheduler state lacks summary"))?;
        let class = match terminal.kind() {
            SchedulerTerminalKind::Completed => ChildTerminalClass::Completed,
            SchedulerTerminalKind::Cancelled => ChildTerminalClass::Cancelled,
            SchedulerTerminalKind::Ambiguous => ChildTerminalClass::Indeterminate,
            SchedulerTerminalKind::Failed
            | SchedulerTerminalKind::DependencyFailed
            | SchedulerTerminalKind::Exhausted => ChildTerminalClass::Failed,
        };
        Ok(Self {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            head: ChildHead::new(
                ChildAggregateKind::Scheduler,
                state.sequence(),
                state.last_event_id(),
                state.state_digest(),
                Some(class),
            )?,
        })
    }

    pub(crate) fn from_wire(
        run_id: RunId,
        revision: RevisionTuple,
        head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        if head.aggregate() != ChildAggregateKind::Scheduler || head.terminal().is_none() {
            return Err(binding("decoded scheduler observation has wrong child kind"));
        }
        Ok(Self { run_id, revision, head })
    }

    #[must_use]
    /// Returns the exact scheduler child run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the scheduler binding revision.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the authoritative terminal scheduler head.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}

/// Checked terminal D3 collaboration observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationChildObservation {
    run_id: RunId,
    revision: RevisionTuple,
    head: ChildHead,
}

impl CollaborationChildObservation {
    /// Projects one terminal collaboration state.
    ///
    /// # Errors
    ///
    /// Returns an error when the state is nonterminal or differs from the current cycle.
    pub fn from_state(
        state: &CollaborationState,
        cycle: &QualityCycleBinding,
    ) -> Result<Self, OrchestratorError> {
        if state.run_id() != cycle.collaboration_run_id()
            || state.binding().id() != cycle.collaboration_id()
            || state.binding().digest() != cycle.collaboration_binding_digest()
            || state.binding().scheduler_id() != cycle.scheduler_id()
            || state.binding().revision() != cycle.revision()
        {
            return Err(binding("terminal collaboration state differs from the current cycle"));
        }
        if state.phase() != CollaborationPhase::Terminal {
            return Err(stale("collaboration observation is not terminal"));
        }
        let terminal = state
            .terminal()
            .ok_or_else(|| binding("terminal collaboration state lacks summary"))?;
        let class = match terminal.kind() {
            CollaborationTerminalKind::Completed => ChildTerminalClass::Completed,
            CollaborationTerminalKind::Cancelled => ChildTerminalClass::Cancelled,
            CollaborationTerminalKind::Failed
            | CollaborationTerminalKind::Abandoned
            | CollaborationTerminalKind::UnsatisfiedJoin => ChildTerminalClass::Failed,
        };
        Ok(Self {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            head: ChildHead::new(
                ChildAggregateKind::Collaboration,
                state.sequence(),
                state.last_event_id(),
                state.state_digest(),
                Some(class),
            )?,
        })
    }

    pub(crate) fn from_wire(
        run_id: RunId,
        revision: RevisionTuple,
        head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        if head.aggregate() != ChildAggregateKind::Collaboration || head.terminal().is_none() {
            return Err(binding("decoded collaboration observation has wrong child kind"));
        }
        Ok(Self { run_id, revision, head })
    }

    #[must_use]
    /// Returns the exact collaboration child run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the collaboration binding revision.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the authoritative terminal collaboration head.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}
