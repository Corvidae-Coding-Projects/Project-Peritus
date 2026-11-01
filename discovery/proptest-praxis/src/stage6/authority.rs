//! Independent one-account budget model and authority-time interval histories.
//! This is discovery-only: synthetic reservation references do not authorize effects.

use std::collections::BTreeMap;

use peritus_budget::{
    Activation, BudgetAccountPhase, BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits,
    BudgetReceiptKind, BudgetRequest, ReservationPhase, ReservationReference,
};
use peritus_policy::{AuthorityInstant, AuthorityTimeState, UseLimit, ValidityWindow};
use peritus_types::{
    AcceptanceSpecId, ActionId, BudgetId, BudgetReservationId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Budget { ceiling: u16, operations: Vec<BudgetOp> },
    Clock { epoch: u8, first: u64, observations: Vec<(u8, u64)> },
    Window { epoch: u8, other_epoch: u8, left: (u64, u64), right: (u64, u64), at: u64 },
    UseBounds { left: Option<u8>, right: Option<u8> },
    TimeAdd { start: u64, delta: u64 },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum BudgetOp {
    Begin { id: u8, consume: u8, reserve: u8 },
    Activate { id: u8 },
    Cancel { id: u8 },
    Settle { id: u8 },
    Seal,
    Close,
}

fn bump(metrics: &mut crate::Metrics, key: &str) {
    *metrics.entry(key.to_owned()).or_default() += 1;
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let op = prop_oneof![
        6 => (1_u8..=8, 0_u8..=25, 0_u8..=25)
            .prop_map(|(id, consume, reserve)| BudgetOp::Begin { id, consume, reserve }),
        2 => (1_u8..=8).prop_map(|id| BudgetOp::Activate { id }),
        2 => (1_u8..=8).prop_map(|id| BudgetOp::Cancel { id }),
        2 => (1_u8..=8).prop_map(|id| BudgetOp::Settle { id }),
        1 => Just(BudgetOp::Seal),
        1 => Just(BudgetOp::Close),
    ];
    let general_budget = (0_u16..=120, prop::collection::vec(op.clone(), 0..=max_steps.min(128)))
        .prop_map(|(ceiling, operations)| Case::Budget { ceiling, operations });
    let active_pattern = prop::collection::vec(op, 0..=max_steps.saturating_sub(3).min(125))
        .prop_map(|mut tail| {
            let mut operations = vec![
                BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
                BudgetOp::Activate { id: 1 },
                BudgetOp::Settle { id: 1 },
            ];
            operations.append(&mut tail);
            Case::Budget { ceiling: 25, operations }
        });
    let replay_pattern = prop::collection::vec(
        (1_u8..=8, 0_u8..=25, 0_u8..=25).prop_map(|(id, consume, reserve)| BudgetOp::Begin {
            id,
            consume,
            reserve,
        }),
        0..=max_steps.saturating_sub(2).min(126),
    )
    .prop_map(|mut tail| {
        let mut operations = vec![
            BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
            BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
        ];
        operations.append(&mut tail);
        Case::Budget { ceiling: 25, operations }
    });
    let cancel_pattern = prop::collection::vec(
        (1_u8..=8, 0_u8..=25, 0_u8..=25).prop_map(|(id, consume, reserve)| BudgetOp::Begin {
            id,
            consume,
            reserve,
        }),
        0..=max_steps.saturating_sub(3).min(125),
    )
    .prop_map(|mut tail| {
        let mut operations = vec![
            BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
            BudgetOp::Cancel { id: 1 },
            BudgetOp::Cancel { id: 1 },
        ];
        operations.append(&mut tail);
        Case::Budget { ceiling: 25, operations }
    });
    prop_oneof![
        8 => general_budget,
        3 => active_pattern,
        2 => replay_pattern,
        2 => cancel_pattern,
        3 => (1_u8..=3, 0_u64..120, prop::collection::vec((1_u8..=3, 0_u64..120), 0..=max_steps.min(128)))
            .prop_map(|(epoch, first, observations)| Case::Clock { epoch, first, observations }),
        3 => (1_u8..=3, 1_u8..=3, (0_u64..150, 0_u64..150), (0_u64..150, 0_u64..150), 0_u64..150)
            .prop_map(|(epoch, other_epoch, left, right, at)| Case::Window { epoch, other_epoch, left, right, at }),
        2 => (1_u8..=3, 0_u64..100, 3_u64..30, 0_u64..3)
            .prop_map(|(epoch, start, span, offset)| Case::Window {
                epoch,
                other_epoch: epoch,
                left: (start, start + span),
                right: (start + offset, start + offset + span),
                at: start + offset,
            }),
        2 => (prop::option::of(0_u8..10), prop::option::of(0_u8..10))
            .prop_map(|(left, right)| Case::UseBounds { left, right }),
        2 => (prop_oneof![Just(u64::MAX), Just(u64::MAX - 1), any::<u64>()],
              prop_oneof![Just(0), Just(1), Just(u64::MAX), any::<u64>()])
            .prop_map(|(start, delta)| Case::TimeAdd { start, delta }),
    ]
    .boxed()
}

fn id_bytes(tag: u8, ordinal: u8) -> [u8; 16] {
    let mut bytes = [tag; 16];
    bytes[15] = ordinal;
    bytes
}

fn root_id() -> BudgetId {
    BudgetId::new(id_bytes(1, 0)).expect("nonzero fixed synthetic id")
}

fn reservation_id(id: u8) -> BudgetReservationId {
    BudgetReservationId::new(id_bytes(2, id)).expect("nonzero synthetic reservation id")
}

fn action_id(id: u8) -> ActionId {
    ActionId::new(id_bytes(3, id)).expect("nonzero synthetic action id")
}

fn digest(tag: u8, id: u8) -> Sha256Digest {
    let mut bytes = [tag; 32];
    bytes[31] = id;
    Sha256Digest::new(bytes)
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(id_bytes(4, 0)).expect("synthetic id"),
        HarnessId::new(id_bytes(5, 0)).expect("synthetic id"),
        WorkspaceId::new(id_bytes(6, 0)).expect("synthetic id"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(id_bytes(7, 0)).expect("synthetic id"),
        ProviderProfileId::new(id_bytes(8, 0)).expect("synthetic id"),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Held,
    Active,
    Settled,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
struct Reservation {
    consume: u8,
    reserve: u8,
    phase: Phase,
}

#[derive(Default)]
struct Model {
    used: u128,
    reserved: u128,
    attempts: u128,
    sealed: bool,
    closed: bool,
    reservations: BTreeMap<u8, Reservation>,
}

fn expected(model: &mut Model, ceiling: u16, operation: BudgetOp) -> Option<BudgetReceiptKind> {
    match operation {
        BudgetOp::Begin { id, consume, reserve } => {
            if let Some(previous) = model.reservations.get(&id) {
                return (previous.consume == consume && previous.reserve == reserve)
                    .then_some(BudgetReceiptKind::Idempotent);
            }
            let requested = u128::from(consume) + u128::from(reserve);
            if model.sealed
                || model.closed
                || model.used + model.reserved + requested > u128::from(ceiling)
            {
                return None;
            }
            model.used += u128::from(consume);
            model.reserved += u128::from(reserve);
            model.attempts += 1;
            model.reservations.insert(
                id,
                Reservation {
                    consume,
                    reserve,
                    phase: if reserve == 0 { Phase::Settled } else { Phase::Held },
                },
            );
            Some(BudgetReceiptKind::Applied)
        }
        BudgetOp::Activate { id } => {
            let record = model.reservations.get_mut(&id)?;
            match record.phase {
                Phase::Held => {
                    record.phase = Phase::Active;
                    Some(BudgetReceiptKind::Applied)
                }
                Phase::Active => Some(BudgetReceiptKind::Idempotent),
                _ => None,
            }
        }
        BudgetOp::Cancel { id } => {
            let record = model.reservations.get_mut(&id)?;
            match record.phase {
                Phase::Held => {
                    record.phase = Phase::Cancelled;
                    model.reserved -= u128::from(record.reserve);
                    Some(BudgetReceiptKind::Applied)
                }
                Phase::Cancelled => Some(BudgetReceiptKind::Idempotent),
                _ => None,
            }
        }
        BudgetOp::Settle { id } => {
            let record = model.reservations.get_mut(&id)?;
            match record.phase {
                Phase::Active => {
                    record.phase = Phase::Settled;
                    model.used += u128::from(record.reserve);
                    model.reserved -= u128::from(record.reserve);
                    Some(BudgetReceiptKind::Applied)
                }
                Phase::Settled if record.reserve != 0 => Some(BudgetReceiptKind::Idempotent),
                _ => None,
            }
        }
        BudgetOp::Seal => {
            if model.sealed {
                Some(BudgetReceiptKind::Idempotent)
            } else {
                model.sealed = true;
                Some(BudgetReceiptKind::Applied)
            }
        }
        BudgetOp::Close => {
            if model.closed {
                return Some(BudgetReceiptKind::Idempotent);
            }
            if !model.sealed
                || model
                    .reservations
                    .values()
                    .any(|r| matches!(r.phase, Phase::Held | Phase::Active))
            {
                return None;
            }
            model.closed = true;
            Some(BudgetReceiptKind::Applied)
        }
    }
}

fn command(operation: BudgetOp, revision: RevisionTuple) -> BudgetCommand {
    match operation {
        BudgetOp::Begin { id, consume, reserve } => BudgetCommand::Begin(BudgetRequest::new(
            reservation_id(id),
            root_id(),
            revision,
            action_id(id),
            digest(20, id),
            BudgetAmounts::from_units(u64::from(consume), 0, 0, 1, 0),
            BudgetAmounts::from_units(u64::from(reserve), 0, 0, 0, 0),
        )),
        BudgetOp::Activate { id } => BudgetCommand::Activate(Activation::new(
            reservation_id(id),
            action_id(id),
            digest(20, id),
            digest(21, id),
        )),
        BudgetOp::Cancel { id } => BudgetCommand::CancelHeld(ReservationReference::new(
            reservation_id(id),
            action_id(id),
            digest(20, id),
            digest(22, id),
        )),
        BudgetOp::Settle { id } => BudgetCommand::SettleExact(ReservationReference::new(
            reservation_id(id),
            action_id(id),
            digest(20, id),
            digest(23, id),
        )),
        BudgetOp::Seal => BudgetCommand::Seal(root_id()),
        BudgetOp::Close => BudgetCommand::Close(root_id()),
    }
}

fn budget(
    ceiling: u16,
    operations: &[BudgetOp],
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if operations.len() > 128 {
        return Err("setup-incomplete: budget operation bound".into());
    }
    let revision = revision();
    let root = root_id();
    let mut ledger = BudgetLedger::new_root(
        root,
        revision,
        BudgetLimits::new(BudgetAmounts::from_units(u64::from(ceiling), 0, 0, 132, 0)),
    );
    let mut model = Model::default();
    for (step, operation) in operations.iter().copied().enumerate() {
        let before = format!("{ledger:?}");
        let wanted = expected(&mut model, ceiling, operation);
        let actual = ledger.transition(command(operation, revision));
        match (wanted, actual) {
            (None, Err(_)) => {
                if format!("{ledger:?}") != before {
                    return Err(format!("rejected budget step {step} mutated ledger"));
                }
                bump(metrics, "budget_rejections");
            }
            (Some(wanted), Ok(transition)) if transition.receipt().kind() == wanted => {
                ledger = transition.into_ledger();
                bump(
                    metrics,
                    if wanted == BudgetReceiptKind::Idempotent {
                        "budget_replays"
                    } else {
                        "budget_applied"
                    },
                );
                if wanted == BudgetReceiptKind::Applied {
                    match operation {
                        BudgetOp::Settle { .. } => bump(metrics, "budget_exact_settlements"),
                        BudgetOp::Cancel { .. } => bump(metrics, "budget_held_cancellations"),
                        _ => {}
                    }
                }
            }
            (wanted, actual) => {
                return Err(format!(
                    "budget step {step} {operation:?}: expected {wanted:?}, observed {actual:?}"
                ));
            }
        }
        ledger
            .validate()
            .map_err(|error| format!("budget step {step} invalid ledger: {error:?}"))?;
        let account = ledger.account(root).map_err(|error| format!("budget account: {error:?}"))?;
        let tokens =
            |amount: BudgetAmounts| amount.get(peritus_budget::BudgetDimension::ModelTokens).get();
        let attempts =
            |amount: BudgetAmounts| amount.get(peritus_budget::BudgetDimension::Attempts).get();
        if u128::from(tokens(account.consumed())) != model.used
            || u128::from(tokens(account.operation_reserved())) != model.reserved
            || u128::from(tokens(account.available()))
                != u128::from(ceiling) - model.used - model.reserved
            || u128::from(attempts(account.consumed())) != model.attempts
            || account.phase()
                != if model.closed {
                    BudgetAccountPhase::Closed
                } else if model.sealed {
                    BudgetAccountPhase::Draining
                } else {
                    BudgetAccountPhase::Open
                }
            || ledger.reservation_count() != model.reservations.len()
        {
            return Err(format!("budget accounting diverged at step {step}: {account:?}"));
        }
        for (&id, record) in &model.reservations {
            let actual = ledger
                .reservation(reservation_id(id))
                .map_err(|error| format!("reservation {id}: {error:?}"))?;
            let phase = match record.phase {
                Phase::Held => ReservationPhase::Held,
                Phase::Active => ReservationPhase::Active,
                Phase::Settled => ReservationPhase::SettledExact,
                Phase::Cancelled => ReservationPhase::CancelledHeld,
            };
            if actual.phase() != phase
                || tokens(actual.outstanding())
                    != if matches!(record.phase, Phase::Held | Phase::Active) {
                        u64::from(record.reserve)
                    } else {
                        0
                    }
            {
                return Err(format!("budget reservation {id} diverged at step {step}: {actual:?}"));
            }
        }
    }
    bump(metrics, "budget_histories");
    Ok(())
}

fn instant(epoch: u8, tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::new(u64::from(epoch)).expect("nonzero generated epoch"), tick)
}

fn clock(
    epoch: u8,
    first: u64,
    observations: &[(u8, u64)],
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if observations.len() > 128 {
        return Err("setup-incomplete: authority clock history bound".into());
    }
    let mut floor = first;
    let mut state = AuthorityTimeState::new(instant(epoch, first));
    for &(candidate_epoch, tick) in observations {
        let accepted = candidate_epoch == epoch && tick >= floor;
        state = match state.observe(instant(candidate_epoch, tick)) {
            Ok(next) if accepted => {
                floor = tick;
                bump(metrics, "clock_advances");
                next
            }
            Err(failure) if !accepted => {
                bump(metrics, "clock_rejections");
                failure.into_state()
            }
            other => {
                return Err(format!(
                    "clock epoch={candidate_epoch} tick={tick} floor={floor}: {other:?}"
                ));
            }
        };
        if state.epoch().get() != u64::from(epoch) || state.greatest_tick_millis() != floor {
            return Err("authority clock floor diverged after observation".into());
        }
    }
    Ok(())
}

fn window(
    epoch: u8,
    other_epoch: u8,
    left: (u64, u64),
    right: (u64, u64),
    at: u64,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let first = ValidityWindow::new(instant(epoch, left.0), instant(epoch, left.1));
    let second = ValidityWindow::new(instant(other_epoch, right.0), instant(other_epoch, right.1));
    if first.is_ok() != (left.0 < left.1) || second.is_ok() != (right.0 < right.1) {
        return Err("validity constructor disagreed with nonempty interval model".into());
    }
    if let (Ok(first), Ok(second)) = (first, second) {
        let contains =
            first.contains(instant(epoch, at)).map_err(|error| format!("contains: {error:?}"))?;
        if contains != (left.0 <= at && at < left.1) {
            return Err("half-open membership diverged".into());
        }
        let within = epoch == other_epoch && left.0 >= right.0 && left.1 <= right.1;
        if first.is_within(second) != within {
            return Err("interval containment diverged".into());
        }
        let low = left.0.max(right.0);
        let high = left.1.min(right.1);
        let intersection = first.intersection(second);
        if epoch != other_epoch || low >= high {
            if intersection.is_ok() {
                return Err("empty or cross-epoch intersection accepted".into());
            }
            bump(metrics, "window_rejections");
        } else {
            let got = intersection.map_err(|error| format!("intersection: {error:?}"))?;
            if got.not_before().tick_millis() != low || got.expires_at().tick_millis() != high {
                return Err("intersection boundaries diverged".into());
            }
            bump(metrics, "window_intersections");
        }
    } else {
        bump(metrics, "window_invalid_inputs");
    }
    Ok(())
}

fn use_limit(value: Option<u8>) -> Option<UseLimit> {
    match value {
        None => Some(UseLimit::unlimited()),
        Some(0) => None,
        Some(count) => UseLimit::limited(u64::from(count)).ok(),
    }
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    let mut metrics = crate::Metrics::new();
    match case {
        Case::Budget { ceiling, operations } => budget(*ceiling, operations, &mut metrics)?,
        Case::Clock { epoch, first, observations } => {
            if *epoch == 0 || observations.iter().any(|(observed_epoch, _)| *observed_epoch == 0) {
                return Err("setup-incomplete: authority epochs are one-based".into());
            }
            clock(*epoch, *first, observations, &mut metrics)?
        }
        Case::Window { epoch, other_epoch, left, right, at } => {
            if *epoch == 0 || *other_epoch == 0 {
                return Err("setup-incomplete: authority epochs are one-based".into());
            }
            window(*epoch, *other_epoch, *left, *right, *at, &mut metrics)?
        }
        Case::UseBounds { left, right } => {
            let a = use_limit(*left);
            let b = use_limit(*right);
            if a.is_some() != (left != &Some(0)) || b.is_some() != (right != &Some(0)) {
                return Err("zero limited use acceptance diverged".into());
            }
            if let (Some(a), Some(b)) = (a, b) {
                let min = match (left, right) {
                    (None, None) => None,
                    (Some(x), None) | (None, Some(x)) => Some(u64::from(*x)),
                    (Some(x), Some(y)) => Some(u64::from((*x).min(*y))),
                };
                if a.intersection(b).remaining() != min
                    || a.is_within(b)
                        != match (left, right) {
                            (_, None) => true,
                            (Some(x), Some(y)) => x <= y,
                            (None, Some(_)) => false,
                        }
                {
                    return Err("use-bound intersection/order diverged".into());
                }
                bump(&mut metrics, "use_bounds_checked");
            } else {
                bump(&mut metrics, "use_bounds_invalid_zero");
            }
        }
        Case::TimeAdd { start, delta } => {
            let result = instant(1, *start).checked_add(*delta);
            let exact = u128::from(*start) + u128::from(*delta);
            if let Ok(sum) = result {
                if exact > u128::from(u64::MAX) || u128::from(sum.tick_millis()) != exact {
                    return Err("time addition wrapped or changed tick".into());
                }
                bump(&mut metrics, "time_additions");
            } else if exact <= u128::from(u64::MAX) {
                return Err("representable time addition rejected".into());
            } else {
                bump(&mut metrics, "time_overflow_rejections");
            }
        }
    }
    Ok(metrics)
}

pub fn calibration() -> Case {
    Case::Budget {
        ceiling: 20,
        operations: vec![
            BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
            BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
        ],
    }
}

pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case::Budget {
            ceiling: 15,
            operations: vec![
                BudgetOp::Begin { id: 1, consume: 2, reserve: 7 },
                BudgetOp::Begin { id: 1, consume: 2, reserve: 7 },
                BudgetOp::Begin { id: 1, consume: 2, reserve: 8 },
                BudgetOp::Cancel { id: 1 },
                BudgetOp::Cancel { id: 1 },
                BudgetOp::Seal,
                BudgetOp::Close,
                BudgetOp::Begin { id: 2, consume: 1, reserve: 1 },
            ],
        },
        Case::Budget {
            ceiling: 0,
            operations: vec![
                BudgetOp::Begin { id: 1, consume: 0, reserve: 0 },
                BudgetOp::Seal,
                BudgetOp::Close,
            ],
        },
        Case::Clock {
            epoch: 2,
            first: 9,
            observations: vec![(2, 9), (2, 12), (1, 40), (2, 11), (2, 15)],
        },
        Case::Window { epoch: 1, other_epoch: 1, left: (3, 11), right: (8, 18), at: 11 },
        Case::Window { epoch: 1, other_epoch: 2, left: (3, 11), right: (8, 18), at: 3 },
        Case::UseBounds { left: None, right: Some(2) },
        Case::UseBounds { left: Some(0), right: Some(2) },
        Case::TimeAdd { start: u64::MAX, delta: 1 },
    ]
}
