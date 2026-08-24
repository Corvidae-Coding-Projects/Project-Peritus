//! `AppContainer` deny-all and managed-filter proxy routing.

use std::net::SocketAddr;

use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// One exact managed proxy route and session-owned WFP policy identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProxyRoute {
    endpoint: SocketAddr,
    routing_handle: u64,
    network_plan_digest: Sha256Digest,
    filter_digest: Sha256Digest,
}

impl ProxyRoute {
    /// Creates a loopback-only proxy route bound to a network plan and native filter.
    ///
    /// # Errors
    /// Rejects a non-loopback/zero-port endpoint, null handle, or zero digest.
    pub fn new(
        endpoint: SocketAddr,
        routing_handle: u64,
        network_plan_digest: Sha256Digest,
        filter_digest: Sha256Digest,
    ) -> Result<Self, WindowsError> {
        if !endpoint.ip().is_loopback()
            || endpoint.port() == 0
            || routing_handle == 0
            || network_plan_digest == Sha256Digest::new([0; 32])
            || filter_digest == Sha256Digest::new([0; 32])
        {
            return Err(WindowsError::new(
                WindowsErrorKind::Network,
                WindowsOperation::Validate,
                WindowsRecovery::CorrectRequest,
                "managed Windows proxy route is incomplete or not loopback",
            ));
        }
        Ok(Self { endpoint, routing_handle, network_plan_digest, filter_digest })
    }

    /// Returns the exact loopback endpoint.
    #[must_use]
    pub const fn endpoint(self) -> SocketAddr {
        self.endpoint
    }

    /// Returns the protected routing-token handle.
    #[must_use]
    pub const fn routing_handle(self) -> u64 {
        self.routing_handle
    }

    /// Returns the stricter C3 network-plan digest.
    #[must_use]
    pub const fn network_plan_digest(self) -> Sha256Digest {
        self.network_plan_digest
    }

    /// Returns the exact installed filter identity.
    #[must_use]
    pub const fn filter_digest(self) -> Sha256Digest {
        self.filter_digest
    }
}

/// Complete Windows network isolation selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkIsolation {
    /// `AppContainer` has no network capabilities.
    DenyAll,
    /// A session-owned dynamic WFP policy permits only this managed proxy route.
    ManagedProxy(ProxyRoute),
}

impl NetworkIsolation {
    /// Returns the optional managed route.
    #[must_use]
    pub const fn proxy(self) -> Option<ProxyRoute> {
        match self {
            Self::DenyAll => None,
            Self::ManagedProxy(route) => Some(route),
        }
    }
}

/// Computes the exact dynamic WFP policy identity for one admitted proxy route.
#[must_use]
pub fn managed_wfp_policy_digest(
    controller: Sha256Digest,
    principal_sid: &str,
    endpoint: SocketAddr,
    network_plan: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-WINDOWS-MANAGED-WFP-POLICY-V1\0".as_slice());
    bytes.extend_from_slice(controller.as_bytes());
    bytes.extend_from_slice(network_plan.as_bytes());
    bytes.extend_from_slice(principal_sid.as_bytes());
    match endpoint.ip() {
        std::net::IpAddr::V4(address) => {
            bytes.push(4);
            bytes.extend_from_slice(&address.octets());
        }
        std::net::IpAddr::V6(address) => {
            bytes.push(6);
            bytes.extend_from_slice(&address.octets());
        }
    }
    bytes.extend_from_slice(&endpoint.port().to_be_bytes());
    peritus_codec::sha256(&bytes)
}
