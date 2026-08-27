//! Durable application-command admission and exact retained response construction.

mod facts;
mod service;

pub(crate) use facts::{committed_result_digest, rejection_result_digest};
pub(crate) use service::submit;
