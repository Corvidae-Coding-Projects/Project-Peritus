//! Constant-memory iteration over deterministic qualification plans.

use crate::{PlanStep, QualificationPlan};

/// Constant-memory iterator over a qualification plan.
pub struct PlanIter<'a> {
    pub(crate) plan: &'a QualificationPlan,
    pub(crate) next: u64,
}

impl Iterator for PlanIter<'_> {
    type Item = PlanStep;

    fn next(&mut self) -> Option<Self::Item> {
        let step = self.plan.step(self.next)?;
        self.next += 1;
        Some(step)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.plan.step_count().saturating_sub(self.next);
        usize::try_from(remaining).map_or((usize::MAX, None), |exact| (exact, Some(exact)))
    }
}

impl<'a> IntoIterator for &'a QualificationPlan {
    type Item = PlanStep;
    type IntoIter = PlanIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
