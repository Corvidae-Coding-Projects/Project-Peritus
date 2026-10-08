//! Exclusive process instance ownership.

mod handoff;
mod lock;
mod record;

pub(crate) use handoff::{
    HandoffCompletion, HandoffOwner, request_handoff, request_legacy_handoff,
};
pub use lock::InstanceGuard;
