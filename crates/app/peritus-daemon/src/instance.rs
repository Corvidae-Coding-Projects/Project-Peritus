//! Exclusive process instance ownership.

mod lock;
mod record;

pub(crate) use lock::InstanceGuard;
