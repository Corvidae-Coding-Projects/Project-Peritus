//! Bounded canonical model-facing tool protocol.
//!
//! Values are validated at construction and contain no ambient effect handles. The router is the
//! sole consumer that turns a [`PreparedToolCall`] into an invocation permit.

mod artifact;
mod call;
mod control;
mod descriptor;
mod envelope;
mod error;
mod identity;
mod limits;
mod prepared;
mod progress;
mod result;
mod schema;
mod verified;
mod wire;

pub use artifact::{ArtifactCompleteness, ArtifactProvenance, ArtifactReference};
pub use call::{CallLimits, ToolCall};
pub use control::{CancellationReason, ControlSet, ToolControl};
pub use descriptor::{
    IdempotencySemantics, LeaseRequirement, ProtocolCompatibility, SideEffectClass, ToolDescriptor,
    ToolLimits,
};
pub use envelope::CanonicalEnvelope;
pub use error::{ProtocolError, ProtocolErrorKind};
pub use identity::{
    BoundedText, IdempotencyKey, ImplementationIdentity, SchemaDigest, SemanticVersion,
};
pub use limits::JsonLimits;
pub use prepared::{PreparedToolCall, ReplayIdentity, prepare_call};
pub use progress::{ProgressKind, ToolProgress};
pub use result::{
    FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, ToolFailure,
    ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
pub use schema::{BoundedJson, Schema, SchemaCompatibility, SchemaProperty};
pub use verified::{
    ProtocolBoundFacts, canonical_order_complete, protocol_bounds_complete, schema_shape_complete,
};
