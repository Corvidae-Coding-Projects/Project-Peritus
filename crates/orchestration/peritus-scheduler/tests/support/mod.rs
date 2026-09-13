#![allow(dead_code, clippy::unwrap_used, reason = "fixed checked test corpus")]

use peritus_scheduler::{
    AttemptNumber, ExecutionClass, RecoveryPolicy, ResourceEntry, ResourceKind, ResourceQuantity,
    ResourceVector, SchedulerBinding, SchedulerCommand, SchedulerCommandKind, SchedulerId,
    SchedulerLimits, SchedulerState, SchedulerTransition, WorkId, WorkSpec, WorkerDescriptor,
    WorkerId, decide, start,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

pub struct Fixture {
    pub limits: SchedulerLimits,
    pub binding: SchedulerBinding,
    pub owner: ActorId,
}

impl Fixture {
    pub fn new() -> Self {
        let limits =
            SchedulerLimits::new(128, 512, 16, 16, 8, 16, 4, 2, 8, 1_048_576, 4_194_304).unwrap();
        let revision = RevisionTuple::new(
            AcceptanceSpecId::new(bytes(10)).unwrap(),
            HarnessId::new(bytes(11)).unwrap(),
            WorkspaceId::new(bytes(12)).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(bytes(13)).unwrap(),
            ProviderProfileId::new(bytes(14)).unwrap(),
        );
        let binding = SchedulerBinding::new(
            RunId::new(bytes(15)).unwrap(),
            SchedulerId::new(bytes(16)).unwrap(),
            revision,
            limits,
            resources(&[(ResourceKind::CPU, 8), (ResourceKind::MEMORY_BYTES, 8_192)], limits),
        )
        .unwrap();
        Self { limits, binding, owner: ActorId::new(bytes(17)).unwrap() }
    }

    pub fn started(&self) -> (SchedulerState, Vec<peritus_scheduler::SchedulerEvent>) {
        let command = SchedulerCommand::new(
            CommandId::new(bytes(1)).unwrap(),
            EventId::new(bytes(2)).unwrap(),
            self.binding.run_id(),
            0,
            None,
            digest(0),
            self.binding.revision(),
            SchedulerCommandKind::StartScheduler { binding: self.binding.clone() },
        )
        .unwrap();
        let transition = start(&command).unwrap();
        (transition.state().clone(), vec![transition.event().clone()])
    }

    pub fn command(
        state: &SchedulerState,
        identity: u8,
        kind: SchedulerCommandKind,
    ) -> SchedulerCommand {
        SchedulerCommand::new(
            CommandId::new(bytes(identity)).unwrap(),
            EventId::new(bytes(identity.wrapping_add(100))).unwrap(),
            state.run_id(),
            state.sequence().get(),
            Some(state.last_event_id()),
            state.state_digest(),
            state.binding().revision(),
            kind,
        )
        .unwrap()
    }

    pub fn apply(
        state: &mut SchedulerState,
        events: &mut Vec<peritus_scheduler::SchedulerEvent>,
        identity: u8,
        kind: SchedulerCommandKind,
    ) -> SchedulerTransition {
        let transition = decide(state, &Self::command(state, identity, kind)).unwrap();
        events.push(transition.event().clone());
        *state = transition.state().clone();
        transition
    }

    pub fn worker(&self, id: u8, concurrency: u16) -> WorkerDescriptor {
        WorkerDescriptor::new(
            WorkerId::new(bytes(id)).unwrap(),
            self.owner,
            vec![ExecutionClass::Tool],
            resources(&[(ResourceKind::CPU, 8), (ResourceKind::MEMORY_BYTES, 8_192)], self.limits),
            concurrency,
            self.limits,
        )
        .unwrap()
    }

    pub fn work(
        &self,
        id: u8,
        priority: u8,
        dependencies: Vec<WorkId>,
        parent: Option<WorkId>,
        maximum_attempts: u16,
        recovery: RecoveryPolicy,
    ) -> WorkSpec {
        WorkSpec::new(
            WorkId::new(bytes(id)).unwrap(),
            self.owner,
            self.binding.revision(),
            ExecutionClass::Tool,
            priority,
            resources(&[(ResourceKind::CPU, 1), (ResourceKind::MEMORY_BYTES, 256)], self.limits),
            None,
            dependencies,
            parent,
            AttemptNumber::new(maximum_attempts).unwrap(),
            recovery,
            digest(id),
            self.limits,
        )
        .unwrap()
    }
}

pub fn resources(values: &[(ResourceKind, u64)], limits: SchedulerLimits) -> ResourceVector {
    ResourceVector::new(
        values
            .iter()
            .map(|(kind, value)| ResourceEntry::new(*kind, ResourceQuantity::new(*value).unwrap()))
            .collect(),
        limits.resource_dimensions(),
    )
    .unwrap()
}

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
