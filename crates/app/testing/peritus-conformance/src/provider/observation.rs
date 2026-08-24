//! Direct capability, stream, retry, accounting, and isolation observations.

mod accounting;
mod capability;
mod recovery;
mod stream;

pub use accounting::{
    ProviderIsolationObservation, ProviderRedactionObservation, ProviderUsageObservation,
    ProviderUsageSnapshot,
};
pub use capability::ProviderCapabilityObservation;
pub use recovery::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderCancellationObservation,
    ProviderFailureObservation, ProviderRetryObservation,
};
pub use stream::{ProviderEventKind, ProviderEventObservation, ProviderStreamObservation};

/// Scenario-specific direct provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderConformanceObservation {
    /// Capability probes.
    Capabilities(ProviderCapabilityObservation),
    /// Streaming/reduction observations.
    Stream(ProviderStreamObservation),
    /// Typed failure observations.
    Failure(ProviderFailureObservation),
    /// Retry and ambiguous-submission observations.
    Retry(ProviderRetryObservation),
    /// Cancellation lifecycle observations.
    Cancellation(ProviderCancellationObservation),
    /// Usage accounting observations.
    Usage(ProviderUsageObservation),
    /// Redaction surfaces.
    Redaction(ProviderRedactionObservation),
    /// Adapter isolation observations.
    Isolation(ProviderIsolationObservation),
}
