//! Exact committed kernel lifecycle and dispatch receipt.

use peritus_journal::{
    CommittedKernelTransition, HeadExpectation, KernelCommitRequest, KernelInputReference,
    SqliteJournal,
};
use peritus_kernel::{CommandEnvelope, KernelAggregate, KernelCommand, ReducerInputs};
use peritus_policy::CapabilityUseTransition;
use peritus_protocol::ActionIntentDto;

use super::{Ids, contract_dto, journal, policy};

pub fn commit(
    store: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
    capability: &CapabilityUseTransition,
) -> CommittedKernelTransition {
    let contract = contract_dto()
        .try_into_domain(peritus_codec::CodecLimits::PRODUCTION)
        .expect("acceptance contract");
    let key = journal::kernel_key(ids.session);
    let envelope =
        CommandEnvelope::new(journal::command(60), journal::event(160), None, ids.revision);
    let genesis =
        KernelAggregate::open(ids.project, ids.session, &contract, ids.revision, envelope)
            .expect("kernel genesis");
    let event = genesis.event();
    let committed = store
        .commit_kernel_transition(
            KernelCommitRequest::genesis(
                journal::kernel_append(envelope, event, HeadExpectation::Absent(key)),
                genesis,
                envelope,
                Vec::new(),
            )
            .expect("bind genesis"),
        )
        .expect("commit genesis");
    let mut state = committed.into_parts().1;
    let (root_budget, child_budget) = policy::budgets(ids);
    state = next(
        store,
        state,
        KernelCommand::StartRun { run_id: ids.run },
        ReducerInputs::new(&contract).with_run_budget(root_budget),
        61,
        None,
    )
    .into_parts()
    .1;
    state = next(
        store,
        state,
        KernelCommand::StartAttempt { run_id: ids.run, attempt_id: ids.attempt },
        ReducerInputs::new(&contract)
            .with_attempt_budget(child_budget)
            .with_parent_budget(root_budget),
        62,
        None,
    )
    .into_parts()
    .1;
    state = next(
        store,
        state,
        KernelCommand::StartTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        ReducerInputs::new(&contract),
        63,
        None,
    )
    .into_parts()
    .1;
    state = next(
        store,
        state,
        intent.propose_command(ids.turn, peritus_codec::CodecLimits::PRODUCTION).expect("proposal"),
        ReducerInputs::new(&contract),
        64,
        None,
    )
    .into_parts()
    .1;
    let reference = KernelInputReference::new(
        1,
        capability.successor().issuance_command_id().as_bytes().to_vec(),
        capability.transition_digest(),
    )
    .expect("capability reference");
    state = next(
        store,
        state,
        KernelCommand::AuthorizeAction { action_id: ids.action },
        ReducerInputs::new(&contract).with_capability_use(capability),
        65,
        Some(reference),
    )
    .into_parts()
    .1;
    next(
        store,
        state,
        KernelCommand::DispatchAction { action_id: ids.action },
        ReducerInputs::new(&contract),
        66,
        None,
    )
}

fn next(
    store: &mut SqliteJournal,
    state: KernelAggregate,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
    seed: u8,
    replay: Option<KernelInputReference>,
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
    let head = store.head(key).expect("kernel head").expect("head present");
    store
        .commit_kernel_transition(
            KernelCommitRequest::transition(
                journal::kernel_append(envelope, event, journal::present(head)),
                transition,
                envelope,
                command,
                replay.into_iter().collect(),
            )
            .expect("bind transition"),
        )
        .expect("commit transition")
}
