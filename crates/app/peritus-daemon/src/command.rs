//! Durable application-command admission and exact retained response construction.

mod facts;
mod recovery;
mod service;
#[cfg(test)]
mod service_tests;

pub use facts::{committed_result_digest, rejection_result_digest};
pub use service::submit;
pub(crate) use recovery::reconcile_record;
