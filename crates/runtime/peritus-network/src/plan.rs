//! Digest-bound runtime projection of a checked C2 network contract.

use peritus_sandbox::{CheckedSandboxPlan, NetworkRule, SecretReference, Transport};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{NetworkError, canonical};

/// How DNS names are resolved for admitted connections.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DnsMode {
    /// The managed proxy performs one fresh system resolution per connection.
    ProxySystem,
}

/// Redirect handling for HTTP forwarding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RedirectMode {
    /// Do not follow redirects inside the managed proxy.
    Deny,
    /// Re-evaluate and follow at most this many redirects.
    Follow {
        /// Maximum successor count.
        maximum: u8,
    },
}

/// Supported managed egress protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProxyMode {
    /// HTTP forwarding and HTTPS CONNECT without TLS interception.
    HttpConnect,
}

/// Complete bounded proxy resource policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NetworkBounds {
    maximum_connections: u16,
    maximum_workers: u16,
    connection_bytes: u64,
    total_bytes: u64,
    connection_millis: u64,
    total_millis: u64,
    observations: u32,
    header_bytes: u32,
}

impl NetworkBounds {
    /// Validates connection, worker, byte, duration, observation, and header ceilings.
    ///
    /// # Errors
    /// Rejects zero, inconsistent, or operationally excessive ceilings.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        maximum_connections: u16,
        maximum_workers: u16,
        connection_bytes: u64,
        total_bytes: u64,
        connection_millis: u64,
        total_millis: u64,
        observations: u32,
        header_bytes: u32,
    ) -> Result<Self, NetworkError> {
        if maximum_connections == 0
            || maximum_connections > 1_024
            || maximum_workers == 0
            || maximum_workers > maximum_connections
            || connection_bytes == 0
            || total_bytes < connection_bytes
            || connection_millis == 0
            || total_millis < connection_millis
            || observations < 4
            || observations > 65_536
            || header_bytes < 256
            || header_bytes > 1024 * 1024
        {
            return Err(crate::error::invalid(
                "network bounds are zero, inconsistent, or excessive",
            ));
        }
        Ok(Self {
            maximum_connections,
            maximum_workers,
            connection_bytes,
            total_bytes,
            connection_millis,
            total_millis,
            observations,
            header_bytes,
        })
    }

    /// Returns the accepted connection ceiling.
    #[must_use]
    pub const fn maximum_connections(self) -> u16 {
        self.maximum_connections
    }
    /// Returns the concurrent worker ceiling.
    #[must_use]
    pub const fn maximum_workers(self) -> u16 {
        self.maximum_workers
    }
    /// Returns the bidirectional byte ceiling for one connection.
    #[must_use]
    pub const fn connection_bytes(self) -> u64 {
        self.connection_bytes
    }
    /// Returns the aggregate bidirectional byte ceiling.
    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }
    /// Returns the duration ceiling for one connection.
    #[must_use]
    pub const fn connection_millis(self) -> u64 {
        self.connection_millis
    }
    /// Returns the lifetime ceiling for the proxy owner.
    #[must_use]
    pub const fn total_millis(self) -> u64 {
        self.total_millis
    }
    /// Returns the retained observation ceiling.
    #[must_use]
    pub const fn observations(self) -> u32 {
        self.observations
    }
    /// Returns the maximum request-header bytes.
    #[must_use]
    pub const fn header_bytes(self) -> u32 {
        self.header_bytes
    }
}

/// C3-only narrowing options applied to the checked C2 plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeNetworkOptions {
    dns: DnsMode,
    redirects: RedirectMode,
    proxy: ProxyMode,
    bounds: NetworkBounds,
    credentials: Vec<SecretReference>,
}

impl RuntimeNetworkOptions {
    /// Creates one runtime narrowing policy.
    #[must_use]
    pub fn new(
        dns: DnsMode,
        redirects: RedirectMode,
        proxy: ProxyMode,
        bounds: NetworkBounds,
        mut credentials: Vec<SecretReference>,
    ) -> Self {
        credentials.sort();
        credentials.dedup();
        Self { dns, redirects, proxy, bounds, credentials }
    }
    /// Returns DNS mode.
    #[must_use]
    pub const fn dns(&self) -> DnsMode {
        self.dns
    }
    /// Returns redirect policy.
    #[must_use]
    pub const fn redirects(&self) -> RedirectMode {
        self.redirects
    }
    /// Returns proxy protocol.
    #[must_use]
    pub const fn proxy(&self) -> ProxyMode {
        self.proxy
    }
    /// Returns resource bounds.
    #[must_use]
    pub const fn bounds(&self) -> NetworkBounds {
        self.bounds
    }
    /// Returns allowed upstream credential references.
    #[must_use]
    pub fn credentials(&self) -> &[SecretReference] {
        &self.credentials
    }
}

/// Canonical runtime network plan derived from one checked C2 plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkPlan {
    owner: ProcessId,
    sandbox_digest: Sha256Digest,
    rules: Vec<NetworkRule>,
    options: RuntimeNetworkOptions,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl NetworkPlan {
    /// Projects a checked C2 plan into an equal-or-narrower managed-network plan.
    ///
    /// # Errors
    /// Rejects UDP requirements because version one has no exact datagram relay, credentials not
    /// present in the checked secret contract, and over-limit canonical data.
    pub fn from_checked(
        checked: &CheckedSandboxPlan,
        options: RuntimeNetworkOptions,
    ) -> Result<Self, NetworkError> {
        if checked
            .requirements()
            .network()
            .iter()
            .any(|target| target.transport() == Transport::Udp)
        {
            return Err(NetworkError::new(
                crate::NetworkErrorKind::Denied,
                crate::NetworkOperation::Compile,
                crate::RecoveryClass::Replan,
                "UDP requires a separately admitted exact datagram relay",
            ));
        }
        for reference in options.credentials() {
            if !checked
                .contract()
                .secrets()
                .grants()
                .iter()
                .any(|grant| grant.reference() == *reference)
            {
                return Err(NetworkError::new(
                    crate::NetworkErrorKind::Credential,
                    crate::NetworkOperation::Compile,
                    crate::RecoveryClass::Replan,
                    "proxy credential reference is absent from checked secret authority",
                ));
            }
        }
        let owner = checked.binding().process_id();
        let sandbox_digest = checked.digest();
        let rules = checked.contract().network().rules().to_vec();
        let canonical = canonical::plan_bytes(owner, sandbox_digest, &rules, &options)?;
        let digest = peritus_codec::sha256(&canonical);
        Ok(Self { owner, sandbox_digest, rules, options, canonical, digest })
    }

    /// Returns the owning process.
    #[must_use]
    pub const fn owner(&self) -> ProcessId {
        self.owner
    }
    /// Returns the source checked sandbox digest.
    #[must_use]
    pub const fn sandbox_digest(&self) -> Sha256Digest {
        self.sandbox_digest
    }
    /// Returns canonical C2 network rules.
    #[must_use]
    pub fn rules(&self) -> &[NetworkRule] {
        &self.rules
    }
    /// Returns runtime narrowing options.
    #[must_use]
    pub const fn options(&self) -> &RuntimeNetworkOptions {
        &self.options
    }
    /// Returns complete canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    /// Returns the runtime plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
