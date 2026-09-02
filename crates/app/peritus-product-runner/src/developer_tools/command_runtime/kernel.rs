//! Exact C0 lifecycle commitments for one command action.

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetSnapshot, ChildBudgetRequest,
};
use peritus_codec::CodecLimits;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, CommittedKernelTransition, HeadExpectation,
    KernelCommitRequest, KernelInputReference, SqliteJournal,
};
use peritus_kernel::{CommandEnvelope, KernelAggregate, KernelCommand, ReducerInputs};
use peritus_policy::CapabilityUseTransition;
use peritus_protocol::ActionIntentDto;
use peritus_spec::AcceptanceContract;

use super::{identity::CommandIds, journal};

#[allow(
    clippy::too_many_lines,
    reason = "the linear C0 lifecycle is easier to audit when its ordered commitments stay together"
)]
pub(super) fn commit(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    contract: &AcceptanceContract,
    intent: &ActionIntentDto,
    capability: &CapabilityUseTransition,
    wall_millis: u64,
) -> Result<CommittedKernelTransition, String> {
    let key = kernel_key(ids)?;
    let envelope = CommandEnvelope::new(
        ids.command("kernel-open-command")?,
        ids.event("kernel-open-event")?,
        None,
        ids.revision,
    );
    let genesis = KernelAggregate::open(ids.project, ids.session, contract, ids.revision, envelope)
        .map_err(|error| format!("open command kernel aggregate: {error:?}"))?;
    let genesis_event = genesis.event();
    let committed = store
        .commit_kernel_transition(
            KernelCommitRequest::genesis(
                journal::kernel_append(
                    ids,
                    store_label,
                    envelope,
                    genesis_event,
                    HeadExpectation::Absent(key),
                )?,
                genesis,
                envelope,
                Vec::new(),
            )
            .map_err(|error| format!("bind command kernel genesis: {error}"))?,
        )
        .map_err(|error| format!("commit command kernel genesis: {error}"))?;
    let mut state = committed.into_parts().1;
    let (root_budget, child_budget) = kernel_budgets(ids, wall_millis)?;
    state = next(
        store,
        store_label,
        ids,
        state,
        KernelCommand::StartRun { run_id: ids.run },
        ReducerInputs::new(contract).with_run_budget(root_budget),
        "kernel-start-run",
        None,
    )?
    .into_parts()
    .1;
    state = next(
        store,
        store_label,
        ids,
        state,
        KernelCommand::StartAttempt { run_id: ids.run, attempt_id: ids.attempt },
        ReducerInputs::new(contract)
            .with_attempt_budget(child_budget)
            .with_parent_budget(root_budget),
        "kernel-start-attempt",
        None,
    )?
    .into_parts()
    .1;
    state = next(
        store,
        store_label,
        ids,
        state,
        KernelCommand::StartTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        ReducerInputs::new(contract),
        "kernel-start-turn",
        None,
    )?
    .into_parts()
    .1;
    state = next(
        store,
        store_label,
        ids,
        state,
        intent
            .propose_command(ids.turn, CodecLimits::PRODUCTION)
            .map_err(|error| format!("construct command action proposal: {error}"))?,
        ReducerInputs::new(contract),
        "kernel-propose-action",
        None,
    )?
    .into_parts()
    .1;
    let reference = KernelInputReference::new(
        1,
        capability.successor().issuance_command_id().as_bytes().to_vec(),
        capability.transition_digest(),
    )
    .map_err(|error| format!("construct command capability reference: {error}"))?;
    let authorized = next(
        store,
        store_label,
        ids,
        state,
        KernelCommand::AuthorizeAction { action_id: ids.action },
        ReducerInputs::new(contract).with_capability_use(capability),
        "kernel-authorize-action",
        Some(reference),
    )?;
    next(
        store,
        store_label,
        ids,
        authorized.into_parts().1,
        KernelCommand::DispatchAction { action_id: ids.action },
        ReducerInputs::new(contract),
        "kernel-dispatch-action",
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn next(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    state: KernelAggregate,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
    label: &str,
    replay: Option<KernelInputReference>,
) -> Result<CommittedKernelTransition, String> {
    let key = kernel_key(ids)?;
    let sequence = state.last_sequence().get().saturating_add(1);
    let envelope = CommandEnvelope::new(
        ids.command(&format!("{label}-command"))?,
        ids.event(&format!("{label}-event"))?,
        Some(state.head_event_id()),
        state.revision(),
    );
    let transition = state
        .reduce(envelope, command.clone(), inputs)
        .into_result()
        .map_err(|error| format!("reduce command kernel transition: {error:?}"))?;
    let event = transition.event();
    let head = store
        .head(key)
        .map_err(|error| format!("load command kernel head: {error}"))?
        .ok_or_else(|| "command kernel head is missing".to_owned())?;
    if event.sequence().get() != sequence {
        return Err("reduced command kernel transition has an unexpected sequence".to_owned());
    }
    store
        .commit_kernel_transition(
            KernelCommitRequest::transition(
                journal::kernel_append(ids, store_label, envelope, event, journal::present(head))?,
                transition,
                envelope,
                command,
                replay.into_iter().collect(),
            )
            .map_err(|error| format!("bind command kernel transition: {error}"))?,
        )
        .map_err(|error| format!("commit command kernel transition: {error}"))
}

fn kernel_key(ids: &CommandIds) -> Result<AggregateKey, String> {
    let aggregate = AggregateId::new(*ids.session.as_bytes())
        .map_err(|error| format!("construct command kernel aggregate: {error}"))?;
    Ok(AggregateKey::new(AggregateKind::Kernel, aggregate))
}

fn kernel_budgets(
    ids: &CommandIds,
    wall_millis: u64,
) -> Result<(BudgetSnapshot, BudgetSnapshot), String> {
    let child_ceiling = wall_millis.saturating_add(5_000);
    let ceiling = child_ceiling.saturating_mul(2);
    let root_limits = BudgetLimits::new(BudgetAmounts::from_units(100, 1_000, ceiling, 10, 5));
    let ledger = BudgetLedger::new_root(ids.kernel_root_budget, ids.revision, root_limits);
    let ledger = ledger
        .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            ids.kernel_child_budget,
            ids.kernel_root_budget,
            ids.revision,
            BudgetLimits::new(BudgetAmounts::from_units(40, 400, child_ceiling, 4, 2)),
        )))
        .map_err(|error| format!("allocate command kernel child budget: {error:?}"))?
        .into_ledger();
    Ok((
        ledger
            .account(ids.kernel_root_budget)
            .map_err(|error| format!("load command root budget: {error:?}"))?,
        ledger
            .account(ids.kernel_child_budget)
            .map_err(|error| format!("load command child budget: {error:?}"))?,
    ))
}
