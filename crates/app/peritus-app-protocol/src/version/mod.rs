//! Protocol version, feature, limit, and compatibility negotiation.

mod feature;
mod negotiation;
mod range;

pub use feature::{ProtocolFeatureName, ProtocolFeatureSet, WellKnownProtocolFeature};
pub use negotiation::{
    ClientHello, ImplementationMetadata, IncompatibilityReason, NegotiatedProtocol,
    NegotiationOutcome, ServerCapabilities, ServerHello, negotiate,
};
pub use range::{ProtocolVersion, ProtocolVersionRange, VersionRange};
