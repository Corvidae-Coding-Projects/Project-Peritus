//! Exact caller-selected elapsed-budget evaluation.

#[doc = " Exhausted caller-selected product-run budget."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetViolation {
    #[doc = " The configured elapsed horizon was exceeded."]
    Elapsed,
}
impl BudgetViolation {
    #[doc = " Reports only an explicitly configured elapsed horizon."]
    pub(crate) const fn from_elapsed(elapsed_exceeded: bool) -> Option<Self> {
        if elapsed_exceeded { Some(Self::Elapsed) } else { None }
    }
}
