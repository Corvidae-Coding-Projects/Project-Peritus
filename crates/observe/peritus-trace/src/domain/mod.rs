//! Closed content-free trace observation domain.

mod attributes;
mod kinds;
mod observation;

pub use attributes::{SafeAttribute, SafeAttributeKey, SafeAttributeValue};
pub use kinds::{DiagnosticCode, ObservationKind, SpanKind, SpanOutcome, StatusCode};
pub use observation::{Observation, ObservedTime};

/// Maximum causal event predecessors on one observation.
pub const MAX_CAUSAL_EVENTS: usize = 64;
/// Maximum safe attributes on one observation.
pub const MAX_SAFE_ATTRIBUTES: usize = 32;
/// Maximum redacted sensitive fields on one observation.
pub const MAX_REDACTED_VALUES: usize = 8;

/// Result of applying an observation to a projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplyOutcome {
    /// A new observation changed the projection.
    Applied,
    /// The same event identity and canonical frame digest were already applied.
    ExactDuplicate,
}
