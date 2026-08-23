//! Boundary tests for fixed-dimensional amount arithmetic.

use peritus_budget::{ArithmeticKind, BudgetAmounts, BudgetDimension, BudgetRecovery};

#[test]
fn fixed_dimensions_are_total_and_stably_ordered() {
    let amounts = BudgetAmounts::from_units(1, 2, 3, 4, 5);
    assert_eq!(amounts.get(BudgetDimension::ModelTokens).get(), 1);
    assert_eq!(amounts.get(BudgetDimension::ProviderCostMicrounits).get(), 2);
    assert_eq!(amounts.get(BudgetDimension::ActiveEffectMilliseconds).get(), 3);
    assert_eq!(amounts.get(BudgetDimension::Attempts).get(), 4);
    assert_eq!(amounts.get(BudgetDimension::Retries).get(), 5);
    assert!(!amounts.is_zero());
    assert!(BudgetAmounts::zero().is_zero());
}

#[test]
fn exact_arithmetic_boundaries_cover_every_dimension() {
    let cases = [
        (
            BudgetDimension::ModelTokens,
            BudgetAmounts::from_units(u64::MAX, 0, 0, 0, 0),
            BudgetAmounts::from_units(1, 0, 0, 0, 0),
        ),
        (
            BudgetDimension::ProviderCostMicrounits,
            BudgetAmounts::from_units(0, u64::MAX, 0, 0, 0),
            BudgetAmounts::from_units(0, 1, 0, 0, 0),
        ),
        (
            BudgetDimension::ActiveEffectMilliseconds,
            BudgetAmounts::from_units(0, 0, u64::MAX, 0, 0),
            BudgetAmounts::from_units(0, 0, 1, 0, 0),
        ),
        (
            BudgetDimension::Attempts,
            BudgetAmounts::from_units(0, 0, 0, u64::MAX, 0),
            BudgetAmounts::from_units(0, 0, 0, 1, 0),
        ),
        (
            BudgetDimension::Retries,
            BudgetAmounts::from_units(0, 0, 0, 0, u64::MAX),
            BudgetAmounts::from_units(0, 0, 0, 0, 1),
        ),
    ];
    for (dimension, maximum, one) in cases {
        let overflow = maximum.checked_add(one).expect_err("must reject overflow");
        assert_eq!(overflow.kind(), ArithmeticKind::Overflow);
        assert_eq!(overflow.dimension(), dimension);
        assert_eq!(overflow.code(), "PERITUS-BUDGET-AMOUNT-001");
        assert_eq!(overflow.recovery(), BudgetRecovery::CallerCorrectable);

        let underflow = BudgetAmounts::zero().checked_sub(one).expect_err("must reject underflow");
        assert_eq!(underflow.kind(), ArithmeticKind::Underflow);
        assert_eq!(underflow.dimension(), dimension);
        assert_eq!(underflow.code(), "PERITUS-BUDGET-AMOUNT-002");
        assert_eq!(underflow.recovery(), BudgetRecovery::CallerCorrectable);
    }
}

#[test]
fn limiting_dimension_set_reports_every_mixed_excess() {
    let request = BudgetAmounts::from_units(11, 9, 31, 1, 2);
    let ceiling = BudgetAmounts::from_units(10, 10, 30, 1, 1);
    let dimensions = request.exceeding_dimensions(ceiling);
    assert!(dimensions.contains(BudgetDimension::ModelTokens));
    assert!(!dimensions.contains(BudgetDimension::ProviderCostMicrounits));
    assert!(dimensions.contains(BudgetDimension::ActiveEffectMilliseconds));
    assert!(!dimensions.contains(BudgetDimension::Attempts));
    assert!(dimensions.contains(BudgetDimension::Retries));
}

#[test]
fn every_stable_error_code_has_an_exact_recovery_contract() {
    use peritus_budget::BudgetErrorKind as Kind;

    let cases = [
        (Kind::UnknownBudget, "PERITUS-BUDGET-001", BudgetRecovery::Terminal),
        (Kind::UnknownReservation, "PERITUS-BUDGET-002", BudgetRecovery::Terminal),
        (Kind::DuplicateBudgetConflict, "PERITUS-BUDGET-003", BudgetRecovery::Terminal),
        (Kind::DuplicateReservationConflict, "PERITUS-BUDGET-004", BudgetRecovery::Terminal),
        (Kind::EmptyRequest, "PERITUS-BUDGET-005", BudgetRecovery::CallerCorrectable),
        (Kind::InvalidAttemptAccounting, "PERITUS-BUDGET-006", BudgetRecovery::CallerCorrectable),
        (Kind::AccountNotOpen, "PERITUS-BUDGET-007", BudgetRecovery::Terminal),
        (Kind::InsufficientBudget, "PERITUS-BUDGET-008", BudgetRecovery::AfterAccountingChange),
        (Kind::InvalidReservationPhase, "PERITUS-BUDGET-009", BudgetRecovery::Terminal),
        (Kind::BindingMismatch, "PERITUS-BUDGET-010", BudgetRecovery::Terminal),
        (Kind::NonmonotonicObservation, "PERITUS-BUDGET-011", BudgetRecovery::Reobserve),
        (Kind::OutstandingWork, "PERITUS-BUDGET-012", BudgetRecovery::AfterAccountingChange),
        (Kind::Arithmetic, "PERITUS-BUDGET-013", BudgetRecovery::CallerCorrectable),
        (Kind::CorruptState, "PERITUS-BUDGET-014", BudgetRecovery::Terminal),
        (Kind::InvalidAccountPhase, "PERITUS-BUDGET-015", BudgetRecovery::Terminal),
        (Kind::PriorAttemptUnresolved, "PERITUS-BUDGET-016", BudgetRecovery::ResolveIndeterminate),
    ];
    for (kind, code, recovery) in cases {
        assert_eq!(kind.code(), code);
        assert_eq!(kind.recovery(), recovery);
    }
}
