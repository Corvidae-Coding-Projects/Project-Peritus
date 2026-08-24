//! Reference resource accounting decisions.

use super::ResourceDecision;
use crate::{ResourceLimits, ResourceUsage, SandboxResourceKind};
use peritus_types::ResourceQuantity;

pub(super) fn charge(
    usage: &mut ResourceUsage,
    limits: &ResourceLimits,
    kind: SandboxResourceKind,
    quantity: ResourceQuantity,
) -> ResourceDecision {
    match usage.charge(kind, quantity, limits) {
        Ok(()) => ResourceDecision::WithinLimit,
        Err(_) => ResourceDecision::LimitExceeded(kind),
    }
}
