//! Digest-bound runtime projection of a checked C2 network contract.

use core::num::NonZeroU64;

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

/// Optional caller-selected proxy lifetime and work policy.
///
/// `None` means that the corresponding logical allowance was not selected. Physical relay
/// buffers, observation pages, worker backpressure, and cancellation polling are implementation
/// windows and are deliberately not represented here.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NetworkBounds {
    maximum_connections: Option<NonZeroU64>,
    maximum_workers: Option<NonZeroU64>,
    connection_bytes: Option<NonZeroU64>,
    total_bytes: Option<NonZeroU64>,
    connection_millis: Option<NonZeroU64>,
    total_millis: Option<NonZeroU64>,
    observations: Option<NonZeroU64>,
    header_bytes: Option<NonZeroU64>,
}

impl NetworkBounds {
    /// Preserves the version-one constructor for callers selecting every legacy policy axis.
    ///
    /// # Errors
    /// Rejects zero or internally inconsistent selected ceilings.
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
            || maximum_workers == 0
            || connection_bytes == 0
            || total_bytes == 0
            || connection_millis == 0
            || total_millis == 0
            || observations == 0
            || header_bytes == 0
        {
            return Err(crate::error::invalid("selected network bounds must be nonzero"));
        }
        Self::from_optional(
            NonZeroU64::new(maximum_connections as u64),
            NonZeroU64::new(maximum_workers as u64),
            NonZeroU64::new(connection_bytes),
            NonZeroU64::new(total_bytes),
            NonZeroU64::new(connection_millis),
            NonZeroU64::new(total_millis),
            NonZeroU64::new(observations as u64),
            NonZeroU64::new(header_bytes as u64),
        )
    }

    /// Creates a policy with no caller-selected logical lifetime or work allowance.
    #[must_use]
    pub const fn without_limits() -> Self {
        Self {
            maximum_connections: None,
            maximum_workers: None,
            connection_bytes: None,
            total_bytes: None,
            connection_millis: None,
            total_millis: None,
            observations: None,
            header_bytes: None,
        }
    }

    /// Creates a policy from independently optional selected limits.
    ///
    /// # Errors
    /// Rejects only contradictions between selected per-connection and aggregate axes.
    #[allow(clippy::too_many_arguments)]
    pub const fn from_optional(
        maximum_connections: Option<NonZeroU64>,
        maximum_workers: Option<NonZeroU64>,
        connection_bytes: Option<NonZeroU64>,
        total_bytes: Option<NonZeroU64>,
        connection_millis: Option<NonZeroU64>,
        total_millis: Option<NonZeroU64>,
        observations: Option<NonZeroU64>,
        header_bytes: Option<NonZeroU64>,
    ) -> Result<Self, NetworkError> {
        if matches!(
            (maximum_connections, maximum_workers),
            (Some(connections), Some(workers)) if workers.get() > connections.get()
        ) || matches!(
            (connection_bytes, total_bytes),
            (Some(connection), Some(total)) if total.get() < connection.get()
        ) || matches!(
            (connection_millis, total_millis),
            (Some(connection), Some(total)) if total.get() < connection.get()
        ) {
            return Err(crate::error::invalid("selected network bounds are inconsistent"));
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

    /// Returns the selected accepted-connection ceiling.
    #[must_use]
    pub const fn maximum_connections(self) -> Option<NonZeroU64> {
        self.maximum_connections
    }
    /// Returns the selected concurrent-worker ceiling.
    #[must_use]
    pub const fn maximum_workers(self) -> Option<NonZeroU64> {
        self.maximum_workers
    }
    /// Returns the selected bidirectional byte ceiling for one connection.
    #[must_use]
    pub const fn connection_bytes(self) -> Option<NonZeroU64> {
        self.connection_bytes
    }
    /// Returns the selected aggregate bidirectional byte ceiling.
    #[must_use]
    pub const fn total_bytes(self) -> Option<NonZeroU64> {
        self.total_bytes
    }
    /// Returns the selected duration ceiling for one connection.
    #[must_use]
    pub const fn connection_millis(self) -> Option<NonZeroU64> {
        self.connection_millis
    }
    /// Returns the selected lifetime ceiling for the proxy owner.
    #[must_use]
    pub const fn total_millis(self) -> Option<NonZeroU64> {
        self.total_millis
    }
    /// Returns the selected retained-observation ceiling.
    #[must_use]
    pub const fn observations(self) -> Option<NonZeroU64> {
        self.observations
    }
    /// Returns the selected per-head byte ceiling.
    #[must_use]
    pub const fn header_bytes(self) -> Option<NonZeroU64> {
        self.header_bytes
    }

    pub(crate) const fn uses_legacy_encoding(self) -> bool {
        match (
            self.maximum_connections,
            self.maximum_workers,
            self.connection_bytes,
            self.total_bytes,
            self.connection_millis,
            self.total_millis,
            self.observations,
            self.header_bytes,
        ) {
            (
                Some(connections),
                Some(workers),
                Some(_),
                Some(_),
                Some(_),
                Some(_),
                Some(observations),
                Some(header_bytes),
            ) => {
                connections.get() <= 1_024
                    && workers.get() <= connections.get()
                    && observations.get() >= 4
                    && observations.get() <= 65_536
                    && header_bytes.get() >= 256
                    && header_bytes.get() <= 1_024 * 1_024
            }
            _ => false,
        }
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
    /// Rejects UDP requirements because no exact datagram relay is available and credentials not
    /// present in the checked secret contract.
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
