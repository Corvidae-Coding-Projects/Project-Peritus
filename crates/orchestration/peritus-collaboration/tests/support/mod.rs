#![allow(dead_code, clippy::unwrap_used, reason = "fixed checked test fixture")]

use peritus_collaboration::{
    CollaborationBinding, CollaborationCommand, CollaborationCommandKind, CollaborationId,
    CollaborationLimits, CollaborationState, CollaborationTaskId, Delegation, JoinPolicy,
    ReservationObservation, decide, start,
};
use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, SchedulerId, WorkId};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

pub struct Fixture {
    pub run_id: RunId,
    pub revision: RevisionTuple,
    pub root_id: CollaborationTaskId,
    pub root_owner: ActorId,
    pub binding: CollaborationBinding,
}

impl Fixture {
    pub fn new(join: JoinPolicy) -> Self {
        let run_id = RunId::new(bytes(1)).unwrap();
        let revision = revision();
        let root_id = CollaborationTaskId::new(bytes(2)).unwrap();
        let root_owner = ActorId::new(bytes(3)).unwrap();
        let root = Delegation::root(
            root_id,
            root_owner,
            HarnessRole::Writer,
            WorkId::new(bytes(4)).unwrap(),
            digest(5),
            join,
        )
        .unwrap();
        let binding = CollaborationBinding::new(
            CollaborationId::new(bytes(6)).unwrap(),
            run_id,
            revision,
            SchedulerId::new(bytes(7)).unwrap(),
            limits(),
            root,
        )
        .unwrap();
        Self { run_id, revision, root_id, root_owner, binding }
    }

    pub fn start(&self) -> (CollaborationState, Vec<peritus_collaboration::CollaborationEvent>) {
        let command = CollaborationCommand::new(
            CommandId::new(bytes(8)).unwrap(),
            EventId::new(bytes(9)).unwrap(),
            self.run_id,
            0,
            None,
            digest(0),
            self.revision,
            CollaborationCommandKind::Start { binding: self.binding.clone() },
        )
        .unwrap();
        let transition = start(&command).unwrap();
        let event = transition.event().clone();
        (transition.into_state(), vec![event])
    }

    pub fn activate_root(
        &self,
        state: CollaborationState,
        events: &mut Vec<peritus_collaboration::CollaborationEvent>,
        seed: u8,
    ) -> CollaborationState {
        apply(
            state,
            events,
            seed,
            CollaborationCommandKind::ActivateTask {
                task_id: self.root_id,
                observation: ReservationObservation::new(
                    self.binding.root_assignment().work_id(),
                    DispatchId::new(bytes(seed.wrapping_add(40))).unwrap(),
                    self.root_owner,
                    self.revision,
                ),
            },
        )
    }

    pub fn child(&self, id: u8, owner: u8, required: bool, join: JoinPolicy) -> Delegation {
        Delegation::child(
            CollaborationTaskId::new(bytes(id)).unwrap(),
            self.root_id,
            self.root_id,
            1,
            ActorId::new(bytes(owner)).unwrap(),
            HarnessRole::Reviewer,
            self.root_owner,
            WorkId::new(bytes(id.wrapping_add(40))).unwrap(),
            digest(id.wrapping_add(80)),
            required,
            join,
        )
        .unwrap()
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "transition tests consume the prior state so stale snapshots cannot be reused"
)]
pub fn apply(
    state: CollaborationState,
    events: &mut Vec<peritus_collaboration::CollaborationEvent>,
    seed: u8,
    kind: CollaborationCommandKind,
) -> CollaborationState {
    let command = command(&state, seed, kind);
    let transition = decide(&state, &command).unwrap();
    events.push(transition.event().clone());
    transition.into_state()
}

pub fn command(
    state: &CollaborationState,
    seed: u8,
    kind: CollaborationCommandKind,
) -> CollaborationCommand {
    CollaborationCommand::new(
        CommandId::new(bytes(seed)).unwrap(),
        EventId::new(bytes(seed.wrapping_add(100))).unwrap(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        kind,
    )
    .unwrap()
}

pub fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(20)).unwrap(),
        HarnessId::new(bytes(21)).unwrap(),
        WorkspaceId::new(bytes(22)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(23)).unwrap(),
        ProviderProfileId::new(bytes(24)).unwrap(),
    )
}

pub fn limits() -> CollaborationLimits {
    CollaborationLimits::new(64, 8, 8, 128, 16, 65_536, 16, 1_048_576, 4_194_304).unwrap()
}

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
