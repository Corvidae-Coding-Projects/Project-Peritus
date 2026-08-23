//! Independent exact typed-error model for rejected trace steps.

use super::{ReferenceModel, TracePoint};
use peritus_budget::{
    ArithmeticKind, BudgetAmounts, BudgetCommand, BudgetDimension, BudgetError, BudgetErrorKind,
};
use peritus_types::{BudgetId, BudgetReservationId};

#[derive(Clone, Copy)]
pub struct ErrorModel {
    kind: BudgetErrorKind,
    budget_id: Option<BudgetId>,
    reservation_id: Option<BudgetReservationId>,
    limiting_dimensions: [bool; 5],
    arithmetic: Option<(ArithmeticKind, BudgetDimension)>,
}

impl ErrorModel {
    fn budget(kind: BudgetErrorKind, budget_id: BudgetId) -> Self {
        Self {
            kind,
            budget_id: Some(budget_id),
            reservation_id: None,
            limiting_dimensions: [false; 5],
            arithmetic: None,
        }
    }

    fn reservation(kind: BudgetErrorKind, reservation_id: BudgetReservationId) -> Self {
        Self {
            kind,
            budget_id: None,
            reservation_id: Some(reservation_id),
            limiting_dimensions: [false; 5],
            arithmetic: None,
        }
    }

    pub fn assert_exact(self, actual: BudgetError, point: &TracePoint) {
        assert_eq!(actual.kind(), self.kind, "{}", point.label());
        assert_eq!(actual.budget_id(), self.budget_id, "{}", point.label());
        assert_eq!(actual.reservation_id(), self.reservation_id, "{}", point.label());
        for (index, dimension) in DIMENSIONS.into_iter().enumerate() {
            assert_eq!(
                actual.limiting_dimensions().contains(dimension),
                self.limiting_dimensions[index],
                "{} dimension {dimension:?}",
                point.label()
            );
        }
        assert_eq!(
            actual.arithmetic_error().map(|error| (error.kind(), error.dimension())),
            self.arithmetic,
            "{}",
            point.label()
        );
    }
}

impl ReferenceModel {
    pub fn rejected(&self, command: BudgetCommand, kind: BudgetErrorKind) -> ErrorModel {
        match kind {
            BudgetErrorKind::UnknownBudget
            | BudgetErrorKind::DuplicateBudgetConflict
            | BudgetErrorKind::InvalidAccountPhase
            | BudgetErrorKind::OutstandingWork => {
                ErrorModel::budget(kind, command_budget_id(command))
            }
            BudgetErrorKind::AccountNotOpen => {
                let mut current = command_budget_id(command);
                loop {
                    let account = self.account(current);
                    if account.phase != peritus_budget::BudgetAccountPhase::Open {
                        return ErrorModel::budget(kind, current);
                    }
                    current = account.parent.expect("non-open modeled ancestor");
                }
            }
            BudgetErrorKind::UnknownReservation
            | BudgetErrorKind::DuplicateReservationConflict
            | BudgetErrorKind::EmptyRequest
            | BudgetErrorKind::InvalidAttemptAccounting
            | BudgetErrorKind::InvalidReservationPhase
            | BudgetErrorKind::PriorAttemptUnresolved
            | BudgetErrorKind::BindingMismatch
            | BudgetErrorKind::NonmonotonicObservation => {
                ErrorModel::reservation(kind, command_reservation_id(command))
            }
            BudgetErrorKind::InsufficientBudget => {
                let BudgetCommand::Begin(request) = command else {
                    panic!("generated insufficient-budget command must be Begin")
                };
                let available = self.account(request.budget_id()).available();
                let requested = request
                    .consume_now()
                    .checked_add(request.reserve())
                    .expect("generated insufficient request must not overflow");
                ErrorModel {
                    kind,
                    budget_id: Some(request.budget_id()),
                    reservation_id: None,
                    limiting_dimensions: dimensions_exceeding(requested, available),
                    arithmetic: None,
                }
            }
            BudgetErrorKind::Arithmetic => {
                let BudgetCommand::Begin(request) = command else {
                    panic!("generated arithmetic command must be Begin")
                };
                ErrorModel {
                    kind,
                    budget_id: None,
                    reservation_id: None,
                    limiting_dimensions: [false; 5],
                    arithmetic: Some((
                        ArithmeticKind::Overflow,
                        first_overflow(request.consume_now(), request.reserve()),
                    )),
                }
            }
            _ => panic!("generated trace needs explicit exact error context for {kind:?}"),
        }
    }
}

const DIMENSIONS: [BudgetDimension; 5] = [
    BudgetDimension::ModelTokens,
    BudgetDimension::ProviderCostMicrounits,
    BudgetDimension::ActiveEffectMilliseconds,
    BudgetDimension::Attempts,
    BudgetDimension::Retries,
];

fn dimensions_exceeding(requested: BudgetAmounts, available: BudgetAmounts) -> [bool; 5] {
    let mut result = [false; 5];
    for (index, dimension) in DIMENSIONS.into_iter().enumerate() {
        result[index] = requested.get(dimension).get() > available.get(dimension).get();
    }
    result
}

fn first_overflow(left: BudgetAmounts, right: BudgetAmounts) -> BudgetDimension {
    DIMENSIONS
        .into_iter()
        .find(|dimension| {
            left.get(*dimension).get().checked_add(right.get(*dimension).get()).is_none()
        })
        .expect("generated arithmetic rejection must overflow")
}

const fn command_budget_id(command: BudgetCommand) -> BudgetId {
    match command {
        BudgetCommand::AllocateChild(request) => request.child_id(),
        BudgetCommand::Begin(request) => request.budget_id(),
        BudgetCommand::Seal(budget_id) | BudgetCommand::Close(budget_id) => budget_id,
        _ => panic!("command does not carry the modeled budget error identity"),
    }
}

const fn command_reservation_id(command: BudgetCommand) -> BudgetReservationId {
    match command {
        BudgetCommand::Begin(request) => request.reservation_id(),
        BudgetCommand::Activate(activation) => activation.reservation_id(),
        BudgetCommand::ObserveUsage(observation) => observation.reservation_id(),
        BudgetCommand::SettleExact(reference) | BudgetCommand::CancelHeld(reference) => {
            reference.reservation_id()
        }
        BudgetCommand::FinalizeAmbiguous(ambiguous) => ambiguous.reference().reservation_id(),
        _ => panic!("command does not carry the modeled reservation error identity"),
    }
}
