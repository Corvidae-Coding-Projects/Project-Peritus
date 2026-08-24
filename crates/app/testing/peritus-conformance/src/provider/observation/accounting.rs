//! Usage, redaction, and adapter-isolation observations.

use crate::ReportText;

/// One provider usage snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderUsageSnapshot {
    input: Option<u64>,
    cached_input: Option<u64>,
    output: Option<u64>,
    total: Option<u64>,
}

impl ProviderUsageSnapshot {
    /// Creates one usage snapshot without synthesizing absent values.
    #[must_use]
    pub const fn new(
        input: Option<u64>,
        cached_input: Option<u64>,
        output: Option<u64>,
        total: Option<u64>,
    ) -> Self {
        Self { input, cached_input, output, total }
    }
    /// Returns input tokens.
    #[must_use]
    pub const fn input(self) -> Option<u64> {
        self.input
    }
    /// Returns cached input tokens.
    #[must_use]
    pub const fn cached_input(self) -> Option<u64> {
        self.cached_input
    }
    /// Returns output tokens.
    #[must_use]
    pub const fn output(self) -> Option<u64> {
        self.output
    }
    /// Returns provider total tokens when supplied.
    #[must_use]
    pub const fn total(self) -> Option<u64> {
        self.total
    }
}

/// Ordered usage observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderUsageObservation(Vec<ProviderUsageSnapshot>);

impl ProviderUsageObservation {
    /// Creates ordered usage observations.
    #[must_use]
    pub const fn new(snapshots: Vec<ProviderUsageSnapshot>) -> Self {
        Self(snapshots)
    }
    /// Returns ordered snapshots.
    #[must_use]
    pub fn snapshots(&self) -> &[ProviderUsageSnapshot] {
        &self.0
    }
}

/// Bounded reportable surfaces checked for one sensitive canary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRedactionObservation {
    sensitive_inputs: u64,
    surfaces: Vec<ReportText>,
}

impl ProviderRedactionObservation {
    /// Creates redaction observations.
    #[must_use]
    pub const fn new(sensitive_inputs: u64, surfaces: Vec<ReportText>) -> Self {
        Self { sensitive_inputs, surfaces }
    }
    /// Returns number of independently injected sensitive values.
    #[must_use]
    pub const fn sensitive_inputs(&self) -> u64 {
        self.sensitive_inputs
    }
    /// Returns bounded reportable surfaces.
    #[must_use]
    pub fn surfaces(&self) -> &[ReportText] {
        &self.surfaces
    }
}

/// Adapter-instance routing observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderIsolationObservation {
    configured_adapter: ReportText,
    request_adapter: ReportText,
    credential_adapter: ReportText,
    transport_adapter: ReportText,
    foreign_transport_requests: u64,
}

impl ProviderIsolationObservation {
    /// Creates adapter-isolation observations.
    #[must_use]
    pub const fn new(
        configured_adapter: ReportText,
        request_adapter: ReportText,
        credential_adapter: ReportText,
        transport_adapter: ReportText,
        foreign_transport_requests: u64,
    ) -> Self {
        Self {
            configured_adapter,
            request_adapter,
            credential_adapter,
            transport_adapter,
            foreign_transport_requests,
        }
    }
    /// Returns configured adapter identity.
    #[must_use]
    pub const fn configured_adapter(&self) -> &ReportText {
        &self.configured_adapter
    }
    /// Returns request-bound adapter identity.
    #[must_use]
    pub const fn request_adapter(&self) -> &ReportText {
        &self.request_adapter
    }
    /// Returns credential-source adapter identity.
    #[must_use]
    pub const fn credential_adapter(&self) -> &ReportText {
        &self.credential_adapter
    }
    /// Returns transport adapter identity.
    #[must_use]
    pub const fn transport_adapter(&self) -> &ReportText {
        &self.transport_adapter
    }
    /// Returns requests observed by the foreign adapter.
    #[must_use]
    pub const fn foreign_transport_requests(&self) -> u64 {
        self.foreign_transport_requests
    }
}
