//! Bounded normalized managed-network observations.

use std::net::IpAddr;

use peritus_sandbox::{DnsName, Transport};
use peritus_types::Sha256Digest;

/// Connection outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionDecision {
    /// Connection was admitted.
    Allowed,
    /// Requested name or IP was denied.
    Denied,
    /// Resolution or connect failed.
    Failed,
    /// A configured ceiling was crossed.
    Limited,
    /// Owner cancellation stopped the connection.
    Cancelled,
}

/// Observation event category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkObservationKind {
    /// Requested destination evaluated.
    Requested,
    /// DNS answer selected.
    Resolved,
    /// Upstream socket connected.
    Connected,
    /// HTTP redirect re-evaluated.
    Redirected,
    /// Scoped credential injected.
    CredentialInjected,
    /// Connection terminated.
    Closed,
    /// Proxy owner and workers released.
    Released,
}

/// One ordered payload-free network observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkObservation {
    sequence: u64,
    plan_digest: Sha256Digest,
    kind: NetworkObservationKind,
    requested_name: Option<DnsName>,
    selected_address: Option<IpAddr>,
    port: Option<u16>,
    transport: Option<Transport>,
    decision: ConnectionDecision,
    redirect_depth: u8,
    uploaded: u64,
    downloaded: u64,
}

impl NetworkObservation {
    /// Creates one normalized observation.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        sequence: u64,
        plan_digest: Sha256Digest,
        kind: NetworkObservationKind,
        requested_name: Option<DnsName>,
        selected_address: Option<IpAddr>,
        port: Option<u16>,
        transport: Option<Transport>,
        decision: ConnectionDecision,
        redirect_depth: u8,
        uploaded: u64,
        downloaded: u64,
    ) -> Self {
        Self {
            sequence,
            plan_digest,
            kind,
            requested_name,
            selected_address,
            port,
            transport,
            decision,
            redirect_depth,
            uploaded,
            downloaded,
        }
    }
    /// Returns sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns runtime plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns category.
    #[must_use]
    pub const fn kind(&self) -> NetworkObservationKind {
        self.kind
    }
    /// Returns requested DNS name if present.
    #[must_use]
    pub const fn requested_name(&self) -> Option<&DnsName> {
        self.requested_name.as_ref()
    }
    /// Returns selected address if present.
    #[must_use]
    pub const fn selected_address(&self) -> Option<IpAddr> {
        self.selected_address
    }
    /// Returns port.
    #[must_use]
    pub const fn port(&self) -> Option<u16> {
        self.port
    }
    /// Returns transport.
    #[must_use]
    pub const fn transport(&self) -> Option<Transport> {
        self.transport
    }
    /// Returns decision.
    #[must_use]
    pub const fn decision(&self) -> ConnectionDecision {
        self.decision
    }
    /// Returns redirect depth.
    #[must_use]
    pub const fn redirect_depth(&self) -> u8 {
        self.redirect_depth
    }
    /// Returns uploaded bytes.
    #[must_use]
    pub const fn uploaded(&self) -> u64 {
        self.uploaded
    }
    /// Returns downloaded bytes.
    #[must_use]
    pub const fn downloaded(&self) -> u64 {
        self.downloaded
    }
}
