//! Stable typed failures for amount arithmetic and ledger transitions.

use crate::{BudgetDimension, BudgetDimensionSet};
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

verus! {

/// Whether exact amount arithmetic overflowed or underflowed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArithmeticKind {
    /// Addition exceeded the `u64` representation.
    Overflow,
    /// Subtraction would have produced a negative amount.
    Underflow,
}

/// A componentwise [`crate::BudgetAmounts`] operation was not representable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmountArithmeticError {
    kind: ArithmeticKind,
    dimension: BudgetDimension,
}

impl AmountArithmeticError {
    pub(crate) const fn overflow(dimension: BudgetDimension) -> (result: Self)
        ensures
            result.spec_kind() == ArithmeticKind::Overflow,
            result.spec_dimension() == dimension,
    {
        Self { kind: ArithmeticKind::Overflow, dimension }
    }

    pub(crate) const fn underflow(dimension: BudgetDimension) -> (result: Self)
        ensures
            result.spec_kind() == ArithmeticKind::Underflow,
            result.spec_dimension() == dimension,
    {
        Self { kind: ArithmeticKind::Underflow, dimension }
    }

    /// Mathematical arithmetic category used by public refinement contracts.
    pub closed spec fn spec_kind(&self) -> ArithmeticKind { self.kind }

    /// Mathematical failed dimension used by public refinement contracts.
    pub closed spec fn spec_dimension(&self) -> BudgetDimension { self.dimension }

    /// Returns the failed arithmetic category.
    #[must_use]
    pub const fn kind(self) -> ArithmeticKind {
        self.kind
    }

    /// Returns the dimension whose exact operation failed.
    #[must_use]
    pub const fn dimension(self) -> BudgetDimension {
        self.dimension
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self.kind {
            ArithmeticKind::Overflow => "PERITUS-BUDGET-AMOUNT-001",
            ArithmeticKind::Underflow => "PERITUS-BUDGET-AMOUNT-002",
        }
    }

    /// Returns the recovery class for this exact arithmetic failure.
    ///
    /// Amount operations are pure and leave both operands unchanged. A caller can therefore
    /// correct the requested quantity or choose a wider budget rather than guessing that a
    /// wrapped or saturated value was accepted.
    #[must_use]
    pub const fn recovery(self) -> BudgetRecovery {
        BudgetRecovery::CallerCorrectable
    }
}

/// Stable category for a rejected budget transition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetErrorKind {
    /// A referenced budget account does not exist.
    UnknownBudget,
    /// A referenced reservation does not exist.
    UnknownReservation,
    /// A budget identity already names a different account.
    DuplicateBudgetConflict,
    /// A reservation identity already names a different operation.
    DuplicateReservationConflict,
    /// A command contains no exact charge and no reservation.
    EmptyRequest,
    /// Attempt and retry charges are not a single pre-execution attempt charge.
    InvalidAttemptAccounting,
    /// The account or one of its ancestors does not admit new work.
    AccountNotOpen,
    /// The requested amounts exceed current available capacity.
    InsufficientBudget,
    /// A reservation is not in the required lifecycle phase.
    InvalidReservationPhase,
    /// An account is not in the required lifecycle phase.
    InvalidAccountPhase,
    /// A fresh retry identity was proposed before its prior attempt reached a terminal state.
    PriorAttemptUnresolved,
    /// An action or evidence binding does not match the reservation.
    BindingMismatch,
    /// A cumulative usage observation moved below its accepted high-water mark.
    NonmonotonicObservation,
    /// A child or account cannot close while it owns live work.
    OutstandingWork,
    /// Exact arithmetic overflowed or underflowed.
    Arithmetic,
    /// Replayed state violates a structural or conservation invariant.
    CorruptState,
}

impl BudgetErrorKind {
    /// Returns the stable subsystem diagnostic code for this category.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnknownBudget => "PERITUS-BUDGET-001",
            Self::UnknownReservation => "PERITUS-BUDGET-002",
            Self::DuplicateBudgetConflict => "PERITUS-BUDGET-003",
            Self::DuplicateReservationConflict => "PERITUS-BUDGET-004",
            Self::EmptyRequest => "PERITUS-BUDGET-005",
            Self::InvalidAttemptAccounting => "PERITUS-BUDGET-006",
            Self::AccountNotOpen => "PERITUS-BUDGET-007",
            Self::InsufficientBudget => "PERITUS-BUDGET-008",
            Self::InvalidReservationPhase => "PERITUS-BUDGET-009",
            Self::BindingMismatch => "PERITUS-BUDGET-010",
            Self::NonmonotonicObservation => "PERITUS-BUDGET-011",
            Self::OutstandingWork => "PERITUS-BUDGET-012",
            Self::Arithmetic => "PERITUS-BUDGET-013",
            Self::CorruptState => "PERITUS-BUDGET-014",
            Self::InvalidAccountPhase => "PERITUS-BUDGET-015",
            Self::PriorAttemptUnresolved => "PERITUS-BUDGET-016",
        }
    }

    /// Returns caller guidance for this category without requiring a forged error value.
    #[must_use]
    pub const fn recovery(self) -> BudgetRecovery {
        match self {
            Self::UnknownBudget
            | Self::UnknownReservation
            | Self::DuplicateBudgetConflict
            | Self::DuplicateReservationConflict
            | Self::BindingMismatch
            | Self::AccountNotOpen
            | Self::InvalidReservationPhase
            | Self::InvalidAccountPhase
            | Self::CorruptState => BudgetRecovery::Terminal,
            Self::EmptyRequest | Self::InvalidAttemptAccounting | Self::Arithmetic => {
                BudgetRecovery::CallerCorrectable
            }
            Self::NonmonotonicObservation => BudgetRecovery::Reobserve,
            Self::PriorAttemptUnresolved => BudgetRecovery::ResolveIndeterminate,
            Self::InsufficientBudget | Self::OutstandingWork => {
                BudgetRecovery::AfterAccountingChange
            }
        }
    }
}

/// Caller guidance associated with a stable budget failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetRecovery {
    /// Repeating the command cannot make it valid.
    Terminal,
    /// The caller must correct the request before trying again.
    CallerCorrectable,
    /// The caller must obtain a current observation before selecting a follow-up command.
    Reobserve,
    /// Outstanding external work must be observed or conservatively resolved.
    ResolveIndeterminate,
    /// The caller may retry only after another accepted accounting transition changes capacity.
    AfterAccountingChange,
}

/// A typed budget failure with bounded machine-readable context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetError {
    kind: BudgetErrorKind,
    budget_id: Option<BudgetId>,
    reservation_id: Option<BudgetReservationId>,
    limiting_dimensions: BudgetDimensionSet,
    arithmetic: Option<AmountArithmeticError>,
}

impl BudgetError {
    pub(crate) const fn budget(kind: BudgetErrorKind, budget_id: BudgetId) -> (result: Self)
        ensures
            result.spec_kind() == kind,
            crate::identity_model::parents_equal(
                result.spec_budget_id(),
                Some(budget_id),
            ),
            result.spec_reservation_id().is_none(),
            result.spec_limiting_dimensions().spec_is_empty(),
            result.spec_arithmetic().is_none(),
    {
        Self {
            kind,
            budget_id: Some(budget_id),
            reservation_id: None,
            limiting_dimensions: BudgetDimensionSet::empty(),
            arithmetic: None,
        }
    }

    pub(crate) const fn reservation(
        kind: BudgetErrorKind,
        reservation_id: BudgetReservationId,
    ) -> (result: Self)
        ensures
            result.spec_kind() == kind,
            result.spec_budget_id().is_none(),
            crate::state::optional_reservation_ids_equal(
                result.spec_reservation_id(),
                Some(reservation_id),
            ),
            result.spec_limiting_dimensions().spec_is_empty(),
            result.spec_arithmetic().is_none(),
    {
        Self {
            kind,
            budget_id: None,
            reservation_id: Some(reservation_id),
            limiting_dimensions: BudgetDimensionSet::empty(),
            arithmetic: None,
        }
    }

    pub(crate) const fn insufficient(
        budget_id: BudgetId,
        limiting_dimensions: BudgetDimensionSet,
    ) -> (result: Self)
        ensures
            result.spec_kind() == BudgetErrorKind::InsufficientBudget,
            crate::identity_model::parents_equal(
                result.spec_budget_id(),
                Some(budget_id),
            ),
            result.spec_reservation_id().is_none(),
            result.spec_limiting_dimensions() == limiting_dimensions,
            result.spec_arithmetic().is_none(),
    {
        Self {
            kind: BudgetErrorKind::InsufficientBudget,
            budget_id: Some(budget_id),
            reservation_id: None,
            limiting_dimensions,
            arithmetic: None,
        }
    }

    pub(crate) const fn arithmetic(error: AmountArithmeticError) -> (result: Self)
        ensures
            result.spec_kind() == BudgetErrorKind::Arithmetic,
            result.spec_budget_id().is_none(),
            result.spec_reservation_id().is_none(),
            result.spec_limiting_dimensions().spec_is_empty(),
            result.spec_arithmetic() == Some(error),
    {
        Self {
            kind: BudgetErrorKind::Arithmetic,
            budget_id: None,
            reservation_id: None,
            limiting_dimensions: BudgetDimensionSet::empty(),
            arithmetic: Some(error),
        }
    }

    pub(crate) closed spec fn spec_kind(&self) -> BudgetErrorKind { self.kind }

    pub(crate) closed spec fn spec_budget_id(&self) -> Option<BudgetId> { self.budget_id }

    pub(crate) closed spec fn spec_reservation_id(&self) -> Option<BudgetReservationId> {
        self.reservation_id
    }

    pub(crate) closed spec fn spec_limiting_dimensions(&self) -> BudgetDimensionSet {
        self.limiting_dimensions
    }

    pub(crate) closed spec fn spec_arithmetic(&self) -> Option<AmountArithmeticError> {
        self.arithmetic
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(self) -> BudgetErrorKind {
        self.kind
    }

    /// Returns the affected budget identity when one is known.
    #[must_use]
    pub const fn budget_id(self) -> Option<BudgetId> {
        self.budget_id
    }

    /// Returns the affected reservation identity when one is known.
    #[must_use]
    pub const fn reservation_id(self) -> Option<BudgetReservationId> {
        self.reservation_id
    }

    /// Returns the dimensions that made a request unavailable.
    #[must_use]
    pub const fn limiting_dimensions(self) -> BudgetDimensionSet {
        self.limiting_dimensions
    }

    /// Returns exact arithmetic context when arithmetic rejected the transition.
    #[must_use]
    pub const fn arithmetic_error(self) -> Option<AmountArithmeticError> {
        self.arithmetic
    }

    /// Returns the stable subsystem diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.kind.code()
    }

    /// Returns the recovery class for this failure.
    #[must_use]
    pub const fn recovery(self) -> BudgetRecovery {
        self.kind.recovery()
    }
}

} // verus!
