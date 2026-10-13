//! Native candidate evaluation and evidence-backed production evolution.

pub mod evolution;
pub mod evaluation;
pub(crate) mod authority;
pub(crate) mod completion;
mod execution;
mod scheduling;
pub mod local;
pub mod production;
pub(crate) mod runtime;
pub(crate) mod recovery;
