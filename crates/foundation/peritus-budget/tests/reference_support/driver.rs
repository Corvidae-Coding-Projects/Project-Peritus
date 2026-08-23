//! Trace inputs, deterministic entropy, and request helpers.

use super::TracePoint;
use crate::support::{Fixture, digest};
use peritus_budget::{BudgetAmounts, BudgetRequest};

pub fn fresh_request(
    fixture: &mut Fixture,
    budget_id: peritus_types::BudgetId,
    digest_byte: u8,
    reserve: BudgetAmounts,
) -> BudgetRequest {
    BudgetRequest::new(
        fixture.reservation_id(),
        budget_id,
        fixture.revision,
        fixture.action_id(),
        digest(digest_byte),
        attempt(false),
        reserve,
    )
}

pub const fn amount(
    tokens: u64,
    cost: u64,
    time: u64,
    attempts: u64,
    retries: u64,
) -> BudgetAmounts {
    BudgetAmounts::from_units(tokens, cost, time, attempts, retries)
}

pub const fn execution(units: u64) -> BudgetAmounts {
    amount(units, units * 2, units * 3, 0, 0)
}

pub const fn attempt(retry: bool) -> BudgetAmounts {
    amount(0, 0, 0, 1, if retry { 1 } else { 0 })
}

pub struct Runner {
    seed: u64,
    case: usize,
    step: usize,
}

impl Runner {
    pub const fn new(seed: u64, case: usize) -> Self {
        Self { seed, case, step: 0 }
    }

    pub const fn next(&mut self) -> TracePoint {
        let point = TracePoint { seed: self.seed, case: self.case, step: self.step };
        self.step += 1;
        point
    }
}

pub struct Generator(u64);

impl Generator {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub const fn bounded(&mut self, upper: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0 % upper
    }
}
