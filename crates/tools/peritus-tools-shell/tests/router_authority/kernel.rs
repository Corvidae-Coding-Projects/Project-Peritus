//! Exact C0 kernel lifecycle and dispatch commitments for C4 routing.

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetSnapshot, ChildBudgetRequest,
};
use peritus_codec::CodecLimits;
use peritus_journal::{
    CommittedKernelTransition, HeadExpectation, KernelCommitRequest, KernelInputReference,
    SqliteJournal,
};
use peritus_kernel::{CommandEnvelope, KernelAggregate, KernelCommand, ReducerInputs};
use peritus_policy::CapabilityUseTransition;
use peritus_protocol::ActionIntentDto;

use super::{Ids, contract_dto, journal};

pub(super) fn commit(
    journal_store: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
    capability: &CapabilityUseTransition,
    dispatch: bool,
) -> CommittedKernelTransition {
    let contract =
        contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
    let key = journal::kernel_key(ids.session);
    let envelope =
        CommandEnvelope::new(journal::command(60), journal::event(160), None, ids.revision);
    let genesis =
        KernelAggregate::open(ids.project, ids.session, &contract, ids.revision, envelope)
            .expect("kernel genesis");
    let genesis_event = genesis.event();
    let committed = journal_store
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
    let (root_budget, child_budget) = kernel_budgets(ids);
    state = next(
        journal_store,
        state,
        KernelCommand::StartRun { run_id: ids.run },
        ReducerInputs::new(&contract).with_run_budget(root_budget),
        61,
        None,
    )
    .into_parts()
    .1;
    state = next(
        journal_store,
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
        journal_store,
        state,
        KernelCommand::StartTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        ReducerInputs::new(&contract),
        63,
        None,
    )
    .into_parts()
    .1;
    state = next(
        journal_store,
        state,
        intent.propose_command(ids.turn, CodecLimits::PRODUCTION).expect("proposal"),
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
    let authorized = next(
        journal_store,
        state,
        KernelCommand::AuthorizeAction { action_id: ids.action },
        ReducerInputs::new(&contract).with_capability_use(capability),
        65,
        Some(reference),
    );
    if !dispatch {
        return authorized;
    }
    next(
        journal_store,
        authorized.into_parts().1,
        KernelCommand::DispatchAction { action_id: ids.action },
        ReducerInputs::new(&contract),
        66,
        None,
    )
}

fn next(
    journal_store: &mut SqliteJournal,
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
    let head = journal_store.head(key).expect("kernel head").expect("kernel head present");
    journal_store
        .commit_kernel_transition(
            KernelCommitRequest::transition(
                journal::kernel_append(envelope, event, journal::present(head)),
                transition,
                envelope,
                command,
                replay_input.into_iter().collect(),
            )
            .expect("bind kernel transition"),
        )
        .expect("commit kernel transition")
}

fn kernel_budgets(ids: &Ids) -> (BudgetSnapshot, BudgetSnapshot) {
    let root_limits = BudgetLimits::new(BudgetAmounts::from_units(100, 1_000, 60_000, 10, 5));
    let ledger = BudgetLedger::new_root(ids.kernel_root_budget, ids.revision, root_limits);
    let ledger = ledger
        .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            ids.kernel_child_budget,
            ids.kernel_root_budget,
            ids.revision,
            BudgetLimits::new(BudgetAmounts::from_units(40, 400, 20_000, 4, 2)),
        )))
        .expect("allocate child budget")
        .into_ledger();
    (
        ledger.account(ids.kernel_root_budget).expect("root budget"),
        ledger.account(ids.kernel_child_budget).expect("child budget"),
    )
}
