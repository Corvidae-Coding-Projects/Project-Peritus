mod journal;
mod policy;
mod workspace;

pub use journal::open as open_journal;
pub use workspace::{
    artifact_store, authorized_patch, intent, mismatched_preimage_patch, receipts, reopen_fixture,
    try_reopen_fixture, try_reopen_fixture_at, workspace_fixture,
};

use std::fs;

use peritus_codec::CodecLimits;
use peritus_journal::{
    AggregateKind, CapabilityCommitRequest, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch, ExpectedAuthorityEpoch, HeadExpectation,
    KernelCommitRequest, KernelInputReference, LeaseCommitRequest, SqliteJournal,
};
use peritus_kernel::{CommandEnvelope, KernelAggregate, KernelCommand, ReducerInputs};
use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseDuration, LeaseHolder, LeaseScope, LeaseTransition,
    LeaseTransitionOutcome, LeaseUseOutcome, MintLease, UseLease,
};
use peritus_policy::{AuthorityInstant, CapabilityUseTransition};
use peritus_protocol::{AcceptanceContractDto, ActionIntentDto};
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, CapabilityName, EnvironmentId, HarnessId, PolicyId,
    ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, RunId, SessionId,
    TurnId, WorkspaceId,
};

pub struct Ids {
    pub workspace: WorkspaceId,
    pub resource: ResourceId,
    pub environment: EnvironmentId,
    pub actor: ActorId,
    pub session: SessionId,
    pub action: ActionId,
    pub capability: CapabilityName,
    pub revision: RevisionTuple,
    project: ProjectId,
    run: RunId,
    attempt: AttemptId,
    turn: TurnId,
    root_budget: BudgetId,
    child_budget: BudgetId,
}

impl Ids {
    pub fn new() -> Self {
        let contract =
            contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
        let workspace = WorkspaceId::new([3; 16]).expect("workspace");
        let revision = RevisionTuple::new(
            contract.id(),
            HarnessId::new([4; 16]).expect("harness"),
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([5; 16]).expect("policy"),
            ProviderProfileId::new([6; 16]).expect("provider"),
        );
        Self {
            workspace,
            resource: ResourceId::new([7; 16]).expect("resource"),
            environment: EnvironmentId::new([8; 16]).expect("environment"),
            actor: ActorId::new([9; 16]).expect("actor"),
            session: SessionId::new([10; 16]).expect("session"),
            action: ActionId::new([11; 16]).expect("action"),
            capability: CapabilityName::new("workspace.mutate".to_owned()).expect("capability"),
            revision,
            project: ProjectId::new([12; 16]).expect("project"),
            run: RunId::new([13; 16]).expect("run"),
            attempt: AttemptId::new([14; 16]).expect("attempt"),
            turn: TurnId::new([15; 16]).expect("turn"),
            root_budget: BudgetId::new([16; 16]).expect("root budget"),
            child_budget: BudgetId::new([17; 16]).expect("child budget"),
        }
    }

    pub const fn holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.actor, self.session)
    }

    pub fn for_action_revision(&self, action_seed: u8, revision: RevisionNumber) -> Self {
        Self {
            workspace: self.workspace,
            resource: self.resource,
            environment: self.environment,
            actor: self.actor,
            session: self.session,
            action: ActionId::new([action_seed; 16]).expect("action"),
            capability: self.capability.clone(),
            revision: RevisionTuple::new(
                self.revision.acceptance_spec_id(),
                self.revision.harness_id(),
                self.workspace,
                self.revision.workspace_generation(),
                revision,
                self.revision.policy_id(),
                self.revision.provider_profile_id(),
            ),
            project: self.project,
            run: self.run,
            attempt: self.attempt,
            turn: self.turn,
            root_budget: self.root_budget,
            child_budget: self.child_budget,
        }
    }
}

pub struct AuthorityReceipts {
    pub kernel: CommittedKernelTransition,
    pub capability: CommittedCapabilityUse,
    pub lease: CommittedLeaseTransition,
    pub epoch: CurrentAuthorityEpoch,
    pub observed_at: AuthorityInstant,
}

pub fn commit_authority(
    journal: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
) -> AuthorityReceipts {
    let action_digest = intent.digest(CodecLimits::PRODUCTION).expect("action digest");
    let capability_use = policy::capability_use(ids, action_digest);
    let kernel = commit_dispatch(journal, ids, intent, &capability_use);
    let (capability, lease) = commit_lease_use(journal, ids, capability_use);
    journal
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
        .expect("allocate current authority epoch");
    let epoch = journal
        .current_authority_epoch()
        .expect("observe authority epoch")
        .expect("authority epoch present");
    AuthorityReceipts { kernel, capability, lease, epoch, observed_at: policy::instant(20) }
}

fn commit_dispatch(
    journal: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
    capability: &CapabilityUseTransition,
) -> CommittedKernelTransition {
    let contract =
        contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
    assert_eq!(contract.id(), ids.revision.acceptance_spec_id());
    let key = journal::kernel_key(ids.session);
    let envelope =
        CommandEnvelope::new(journal::command(60), journal::event(160), None, ids.revision);
    let genesis =
        KernelAggregate::open(ids.project, ids.session, &contract, ids.revision, envelope)
            .expect("kernel genesis");
    let genesis_event = genesis.event();
    let committed = journal
        .commit_kernel_transition(
            KernelCommitRequest::genesis(
                journal::kernel_append(envelope, genesis_event, HeadExpectation::Absent(key)),
                genesis,
                envelope,
                Vec::new(),
            )
            .expect("bind genesis"),
        )
        .expect("commit genesis");
    let mut state = committed.into_parts().1;
    let (root_budget, child_budget) = policy::budgets(ids);
    state = commit_next(
        journal,
        state,
        KernelCommand::StartRun { run_id: ids.run },
        ReducerInputs::new(&contract).with_run_budget(root_budget),
        61,
    )
    .into_parts()
    .1;
    state = commit_next(
        journal,
        state,
        KernelCommand::StartAttempt { run_id: ids.run, attempt_id: ids.attempt },
        ReducerInputs::new(&contract)
            .with_attempt_budget(child_budget)
            .with_parent_budget(root_budget),
        62,
    )
    .into_parts()
    .1;
    state = commit_next(
        journal,
        state,
        KernelCommand::StartTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        ReducerInputs::new(&contract),
        63,
    )
    .into_parts()
    .1;
    state = commit_next(
        journal,
        state,
        intent.propose_command(ids.turn, CodecLimits::PRODUCTION).expect("proposal"),
        ReducerInputs::new(&contract),
        64,
    )
    .into_parts()
    .1;
    let capability_reference = KernelInputReference::new(
        1,
        capability.successor().issuance_command_id().as_bytes().to_vec(),
        capability.transition_digest(),
    )
    .expect("capability replay reference");
    state = commit_next_with_reference(
        journal,
        state,
        KernelCommand::AuthorizeAction { action_id: ids.action },
        ReducerInputs::new(&contract).with_capability_use(capability),
        65,
        Some(capability_reference),
    )
    .into_parts()
    .1;
    commit_next(
        journal,
        state,
        KernelCommand::DispatchAction { action_id: ids.action },
        ReducerInputs::new(&contract),
        66,
    )
}

fn commit_next(
    journal: &mut SqliteJournal,
    state: KernelAggregate,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
    seed: u8,
) -> CommittedKernelTransition {
    commit_next_with_reference(journal, state, command, inputs, seed, None)
}

fn commit_next_with_reference(
    journal: &mut SqliteJournal,
    state: KernelAggregate,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
    seed: u8,
    replay_input: Option<KernelInputReference>,
) -> CommittedKernelTransition {
    let key = journal::kernel_key(state.session().id());
    let envelope = CommandEnvelope::new(
        journal::command(seed),
        journal::event(seed.wrapping_add(100)),
        Some(state.head_event_id()),
        state.revision(),
    );
    let transition =
        state.reduce(envelope, command.clone(), inputs).into_result().expect("kernel transition");
    let event = transition.event();
    let head = journal.head(key).expect("kernel head").expect("kernel head present");
    let inputs = replay_input.into_iter().collect();
    journal
        .commit_kernel_transition(
            KernelCommitRequest::transition(
                journal::kernel_append(envelope, event, journal::present(head)),
                transition,
                envelope,
                command,
                inputs,
            )
            .expect("bind kernel transition"),
        )
        .expect("commit kernel transition")
}

fn commit_lease_use(
    journal: &mut SqliteJournal,
    ids: &Ids,
    capability: CapabilityUseTransition,
) -> (CommittedCapabilityUse, CommittedLeaseTransition) {
    let scope = LeaseScope::new(ids.workspace, ids.resource, ids.environment);
    let key = journal::aggregate(AggregateKind::Lease, 40);
    let mint =
        LeaseAggregate::mint(MintLease::new(journal::command(40), scope, policy::instant(10)))
            .expect("mint lease");
    let minted = commit_lease(journal, ids, key, mint, 1, 40, None);
    let active = accepted(minted.into_parts().1.acquire(AcquireLease::new(
        journal::command(41),
        ids.holder(),
        LeaseDuration::new(50).expect("lease duration"),
        policy::instant(10),
    )));
    let acquired = commit_lease(journal, ids, key, active, 2, 41, Some(journal::event(40)));
    let active = acquired.into_parts().1;
    let claim = active.active().expect("active lease").claim();
    let logical = match active.authorize_use(UseLease::new(
        journal::command(42),
        claim,
        policy::instant(20),
        capability,
    )) {
        LeaseUseOutcome::Accepted(value) => value,
        LeaseUseOutcome::Rejected(failure) => panic!("lease use: {:?}", failure.error()),
    };
    let (lease_transition, capability_transition) = logical.into_parts();
    let capability_key = journal::aggregate(AggregateKind::Approval, 70);
    let capability = journal
        .commit_capability_use(
            CapabilityCommitRequest::new(
                journal::append(
                    capability_key,
                    journal::command(43),
                    1,
                    journal::event(43),
                    None,
                    HeadExpectation::Absent(capability_key),
                    ids.revision,
                ),
                capability_transition,
                None,
            )
            .expect("bind capability use"),
        )
        .expect("commit capability use");
    let lease = commit_lease(journal, ids, key, lease_transition, 3, 42, Some(journal::event(41)));
    (capability, lease)
}

fn commit_lease(
    journal: &mut SqliteJournal,
    ids: &Ids,
    key: peritus_journal::AggregateKey,
    transition: LeaseTransition,
    sequence: u64,
    seed: u8,
    previous: Option<peritus_types::EventId>,
) -> CommittedLeaseTransition {
    let head = journal
        .head(key)
        .expect("lease head")
        .map_or(HeadExpectation::Absent(key), HeadExpectation::Present);
    journal
        .commit_lease_transition(
            LeaseCommitRequest::new(
                journal::append(
                    key,
                    journal::command(seed),
                    sequence,
                    journal::event(seed),
                    previous,
                    head,
                    ids.revision,
                ),
                transition,
            )
            .expect("bind lease transition"),
        )
        .expect("commit lease transition")
}

fn accepted(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => value,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("lease transition: {:?}", failure.error())
        }
    }
}

fn contract_dto() -> AcceptanceContractDto {
    let current = std::env::current_dir().expect("test working directory");
    let path = current
        .ancestors()
        .map(|root| root.join("protocol/fixtures/v1/acceptance-contract.bin"))
        .find(|path| path.is_file())
        .expect("checked-in acceptance contract path");
    let bytes = fs::read(path).expect("checked-in acceptance contract");
    peritus_codec::decode_message(&bytes, CodecLimits::PRODUCTION).expect("contract DTO")
}
