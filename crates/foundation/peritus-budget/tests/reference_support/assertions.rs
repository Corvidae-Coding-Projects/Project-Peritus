//! Exact production-ledger comparisons for the independent reference state.

use super::{ReferenceModel, TracePoint, Units};
use peritus_budget::{BudgetAmounts, BudgetLedger};

impl ReferenceModel {
    pub fn assert_matches(&self, ledger: &BudgetLedger, point: &TracePoint) {
        assert_eq!(ledger.root_id(), self.accounts[0].id, "{} root", point.label());
        assert_eq!(ledger.account_count(), self.accounts.len(), "{} accounts", point.label());
        assert_eq!(
            ledger.reservation_count(),
            self.reservations.len(),
            "{} reservations",
            point.label()
        );
        self.assert_accounts(ledger, point);
        self.assert_reservations(ledger, point);
        ledger.validate().unwrap_or_else(|error| panic!("{} validation: {error:?}", point.label()));
    }

    fn assert_accounts(&self, ledger: &BudgetLedger, point: &TracePoint) {
        for expected in &self.accounts {
            let actual = ledger
                .account(expected.id)
                .unwrap_or_else(|error| panic!("{} account lookup: {error:?}", point.label()));
            assert_eq!(actual.id(), expected.id, "{}", point.label());
            assert_eq!(actual.parent_id(), expected.parent, "{}", point.label());
            assert_eq!(actual.revision(), expected.revision, "{}", point.label());
            assert_eq!(actual.limits().amounts(), expected.limit.amount(), "{}", point.label());
            assert_eq!(actual.consumed(), expected.consumed.amount(), "{}", point.label());
            assert_eq!(
                actual.operation_reserved(),
                expected.reserved.amount(),
                "{}",
                point.label()
            );
            assert_eq!(
                actual.child_delegated_remaining(),
                expected.delegated.amount(),
                "{}",
                point.label()
            );
            assert_eq!(actual.available(), expected.available(), "{}", point.label());
            assert_eq!(actual.phase(), expected.phase, "{}", point.label());
        }
    }

    fn assert_reservations(&self, ledger: &BudgetLedger, point: &TracePoint) {
        for expected in &self.reservations {
            let actual = ledger
                .reservation(expected.request.reservation_id())
                .unwrap_or_else(|error| panic!("{} reservation lookup: {error:?}", point.label()));
            assert_eq!(actual.request(), expected.request, "{}", point.label());
            assert_eq!(actual.observed(), expected.observed.amount(), "{}", point.label());
            assert_eq!(actual.outstanding(), expected.outstanding(), "{}", point.label());
            assert_eq!(actual.phase(), expected.phase, "{}", point.label());
            assert_eq!(actual.activation_evidence(), expected.activation, "{}", point.label());
            assert_eq!(actual.observation_evidence(), expected.observation, "{}", point.label());
            assert_eq!(actual.final_evidence(), expected.final_evidence, "{}", point.label());
            assert_eq!(
                actual.final_reported(),
                expected.final_reported.map(Units::amount),
                "{}",
                point.label()
            );
            assert_eq!(actual.finality(), expected.finality, "{}", point.label());
        }
    }
}

impl super::AccountModel {
    pub(super) fn available(&self) -> BudgetAmounts {
        self.limit
            .subtracted(self.consumed)
            .subtracted(self.reserved)
            .subtracted(self.delegated)
            .amount()
    }
}

impl super::ReservationModel {
    pub(super) fn outstanding(&self) -> BudgetAmounts {
        if matches!(
            self.phase,
            peritus_budget::ReservationPhase::Held | peritus_budget::ReservationPhase::Active
        ) {
            Units::from_amount(self.request.reserve()).subtracted(self.observed).amount()
        } else {
            BudgetAmounts::zero()
        }
    }
}

impl TracePoint {
    pub(super) fn label(&self) -> String {
        format!("seed {:#x} case {} step {}", self.seed, self.case, self.step)
    }
}
