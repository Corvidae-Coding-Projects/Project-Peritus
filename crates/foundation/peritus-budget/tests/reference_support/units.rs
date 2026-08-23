//! Independent fixed-vector arithmetic used by the trace model.

use peritus_budget::{BudgetAmounts, BudgetDimension};

#[derive(Clone, Copy)]
pub(super) struct Units([u64; 5]);

impl Units {
    pub(super) const fn zero() -> Self {
        Self([0; 5])
    }

    pub(super) const fn from_amount(amount: BudgetAmounts) -> Self {
        Self([
            amount.get(BudgetDimension::ModelTokens).get(),
            amount.get(BudgetDimension::ProviderCostMicrounits).get(),
            amount.get(BudgetDimension::ActiveEffectMilliseconds).get(),
            amount.get(BudgetDimension::Attempts).get(),
            amount.get(BudgetDimension::Retries).get(),
        ])
    }

    pub(super) const fn amount(self) -> BudgetAmounts {
        BudgetAmounts::from_units(self.0[0], self.0[1], self.0[2], self.0[3], self.0[4])
    }

    pub(super) const fn is_zero(self) -> bool {
        self.0[0] | self.0[1] | self.0[2] | self.0[3] | self.0[4] == 0
    }

    pub(super) fn add(&mut self, other: Self) {
        for index in 0..5 {
            self.0[index] = self.0[index].checked_add(other.0[index]).expect("model add");
        }
    }

    pub(super) fn sub(&mut self, other: Self) {
        for index in 0..5 {
            self.0[index] = self.0[index].checked_sub(other.0[index]).expect("model sub");
        }
    }

    pub(super) fn subtracted(self, other: Self) -> Self {
        let mut result = self;
        result.sub(other);
        result
    }
}
