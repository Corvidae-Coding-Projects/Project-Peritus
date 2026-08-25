//! Durable causal tracing and redaction boundary for Peritus.
//!
//! Trace observations are inert facts. They can describe authoritative work but cannot authorize,
//! dispatch, accept, cancel, retry, or otherwise mutate that work. Default values are deliberately
//! closed and content-free; sensitive bytes must pass through [`redact_sensitive`].

mod binding;
mod codec;
mod domain;
mod error;
mod format;
mod identity;
mod projection;
mod recovery;
mod redaction;
mod storage;
pub mod verified;

pub use binding::CausalBinding;
pub use domain::{
    ApplyOutcome, DiagnosticCode, Observation, ObservationKind, ObservedTime, SafeAttribute,
    SafeAttributeKey, SafeAttributeValue, SpanKind, SpanOutcome, StatusCode,
};
pub use error::{RecoveryClass, TraceError, TraceErrorKind};
pub use format::{TRACE_OBSERVATION_FAMILY, TRACE_OBSERVATION_SCHEMA, trace_schema_digest};
pub use identity::{SpanId, TraceId};
pub use projection::{
    ProjectedObservation, SpanSnapshot, TraceProjection, TraceProjectionState, TraceSnapshot,
};
pub use recovery::{RecoveredTrace, recover_all, recover_trace};
pub use redaction::{
    ArtifactVaultReference, RedactedValue, SensitivePayload, SensitivityClass, redact_sensitive,
};
pub use storage::{JournalTraceStore, RecordedObservation};
