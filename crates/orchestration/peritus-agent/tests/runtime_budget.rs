//! B1 model/tool reservation integration at the D0 runtime boundary.

mod budget_fixture;

use budget_fixture::LedgerBudgetPort;
use peritus_agent::{AgentBudgetError, AgentBudgetPlan, AgentBudgetReservation, AgentBudgetState};
use peritus_budget::{BudgetAmounts, UsageFinality};
use peritus_model_protocol::UsageCounters;
use peritus_types::{
    AcceptanceSpecId, ActionId, BudgetId, BudgetReservationId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn model_attempt_charges_attempt_tokens_cost_and_time_then_admits_one_retry() {
    let revision = revision();
    let root = budget_id(1);
    let action = action_id(2);
    let action_digest = digest(3);
    let mut port =
        LedgerBudgetPort::new(root, revision, BudgetAmounts::from_units(100, 50, 1_000, 2, 1));
    let first = plan(
        4,
        root,
        revision,
        action,
        action_digest,
        BudgetAmounts::from_units(60, 30, 600, 0, 0),
        false,
    );
    let mut reservation = AgentBudgetReservation::begin(&mut port, first).expect("reserve");
    reservation.activate(&mut port, digest(5)).expect("activate");
    reservation
        .observe_model(
            &mut port,
            digest(6),
            UsageCounters::new(Some(12), None, None, Some(8), None, None, Some(20), Some(5)),
            100,
            UsageFinality::Interim,
        )
        .expect("interim usage");
    reservation
        .observe_model(
            &mut port,
            digest(7),
            UsageCounters::new(Some(25), None, None, Some(15), None, None, Some(40), Some(10)),
            300,
            UsageFinality::Final,
        )
        .expect("final usage");
    assert_eq!(reservation.state(), AgentBudgetState::Settled);
    assert_eq!(
        port.ledger().account(root).expect("account").consumed(),
        BudgetAmounts::from_units(40, 10, 300, 1, 0)
    );

    let retry = plan(
        8,
        root,
        revision,
        action,
        action_digest,
        BudgetAmounts::from_units(20, 5, 100, 0, 0),
        true,
    );
    AgentBudgetReservation::begin(&mut port, retry).expect("retry reservation");
    assert_eq!(
        port.ledger().account(root).expect("account").consumed(),
        BudgetAmounts::from_units(40, 10, 300, 2, 1)
    );
}

#[test]
fn missing_final_usage_stays_active_until_conservative_ambiguity_consumes_the_ceiling() {
    let revision = revision();
    let root = budget_id(20);
    let mut port =
        LedgerBudgetPort::new(root, revision, BudgetAmounts::from_units(50, 10, 100, 1, 0));
    let mut reservation = AgentBudgetReservation::begin(
        &mut port,
        plan(
            21,
            root,
            revision,
            action_id(22),
            digest(23),
            BudgetAmounts::from_units(50, 10, 100, 0, 0),
            false,
        ),
    )
    .expect("reserve");
    reservation.activate(&mut port, digest(24)).expect("activate");
    let error = reservation
        .observe_model(&mut port, digest(25), UsageCounters::default(), 40, UsageFinality::Final)
        .expect_err("unknown final usage cannot release capacity");
    assert_eq!(error, AgentBudgetError::IncompleteFinalUsage);
    assert_eq!(reservation.state(), AgentBudgetState::Active);
    reservation.finalize_ambiguous(&mut port, digest(26)).expect("conservative finalization");
    assert_eq!(reservation.state(), AgentBudgetState::Indeterminate);
    assert_eq!(
        port.ledger().account(root).expect("account").consumed(),
        BudgetAmounts::from_units(50, 10, 100, 1, 0)
    );
}

#[test]
fn tool_effect_time_uses_the_same_verified_b1_lifecycle() {
    let revision = revision();
    let root = budget_id(40);
    let mut port =
        LedgerBudgetPort::new(root, revision, BudgetAmounts::from_units(0, 0, 500, 1, 0));
    let mut reservation = AgentBudgetReservation::begin(
        &mut port,
        plan(
            41,
            root,
            revision,
            action_id(42),
            digest(43),
            BudgetAmounts::from_units(0, 0, 500, 0, 0),
            false,
        ),
    )
    .expect("reserve");
    reservation.activate(&mut port, digest(44)).expect("activate");
    reservation
        .observe_effect(&mut port, digest(45), 125, UsageFinality::Final)
        .expect("terminal tool use");
    assert_eq!(
        port.ledger().account(root).expect("account").consumed(),
        BudgetAmounts::from_units(0, 0, 125, 1, 0)
    );
}

#[allow(clippy::too_many_arguments)]
fn plan(
    reservation: u8,
    budget: BudgetId,
    revision: RevisionTuple,
    action: ActionId,
    action_digest: Sha256Digest,
    ceiling: BudgetAmounts,
    retry: bool,
) -> AgentBudgetPlan {
    AgentBudgetPlan::new(
        BudgetReservationId::new([reservation; 16]).expect("reservation"),
        budget,
        revision,
        action,
        action_digest,
        ceiling,
        retry,
    )
    .expect("plan")
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([51; 16]).expect("acceptance"),
        HarnessId::new([52; 16]).expect("harness"),
        WorkspaceId::new([53; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([54; 16]).expect("policy"),
        ProviderProfileId::new([55; 16]).expect("provider"),
    )
}

fn budget_id(seed: u8) -> BudgetId {
    BudgetId::new([seed; 16]).expect("budget")
}

fn action_id(seed: u8) -> ActionId {
    ActionId::new([seed; 16]).expect("action")
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
