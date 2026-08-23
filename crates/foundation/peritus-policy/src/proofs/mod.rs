//! Proof obligations for compiled separation, exact scope, restriction monotonicity, and reducers.

mod monotonicity;
mod query_bridge;
mod refinement;
mod role_separation;
mod scope;

#[cfg(verus_only)]
pub(crate) use monotonicity::evaluator_cannot_broaden_allowed_queries;
