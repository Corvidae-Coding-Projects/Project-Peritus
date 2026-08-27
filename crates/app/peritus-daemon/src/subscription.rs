//! Canonical topic compilation and bounded live event delivery.

mod pump;
mod registry;

pub(crate) use pump::SubscriptionRegistry;
