//! Capability-probe observations.

use super::super::ProviderCapability;

/// Capability probes and transport effects observed from one immutable profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCapabilityObservation {
    advertised: Vec<ProviderCapability>,
    succeeded: Vec<ProviderCapability>,
    rejected_before_transport: Vec<ProviderCapability>,
    encoded: Vec<ProviderCapability>,
    transport_requests: u64,
}

impl ProviderCapabilityObservation {
    /// Creates one complete capability observation.
    #[must_use]
    pub const fn new(
        advertised: Vec<ProviderCapability>,
        succeeded: Vec<ProviderCapability>,
        rejected_before_transport: Vec<ProviderCapability>,
        encoded: Vec<ProviderCapability>,
        transport_requests: u64,
    ) -> Self {
        Self { advertised, succeeded, rejected_before_transport, encoded, transport_requests }
    }

    /// Returns advertised supported features.
    #[must_use]
    pub fn advertised(&self) -> &[ProviderCapability] {
        &self.advertised
    }
    /// Returns features whose positive probes completed.
    #[must_use]
    pub fn succeeded(&self) -> &[ProviderCapability] {
        &self.succeeded
    }
    /// Returns unsupported features rejected before transport.
    #[must_use]
    pub fn rejected_before_transport(&self) -> &[ProviderCapability] {
        &self.rejected_before_transport
    }
    /// Returns features observed in encoded requests.
    #[must_use]
    pub fn encoded(&self) -> &[ProviderCapability] {
        &self.encoded
    }
    /// Returns exact transport request count across all probes.
    #[must_use]
    pub const fn transport_requests(&self) -> u64 {
        self.transport_requests
    }
}
