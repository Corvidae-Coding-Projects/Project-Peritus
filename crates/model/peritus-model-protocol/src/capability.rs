//! Explicit provider capability profiles and pure negotiation.

mod feature;
mod negotiation;
mod profile;

pub use feature::{Capability, CapabilityMatrix, CapabilityState};
pub use negotiation::{NegotiatedCapabilities, RequestedCapabilities, negotiate};
pub use profile::{
    CancellationKind, CapabilityProvenance, ModelLimits, OutputLimitEnforcement, ProviderProfile,
    ResumeKind, StateMode, WireDialect,
};
