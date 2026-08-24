//! Network namespace and managed-proxy route projection.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use std::net::SocketAddr;

/// Exact managed proxy endpoint visible from the child network namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProxyRoute {
    endpoint: SocketAddr,
}

impl ProxyRoute {
    /// Creates an exact IP endpoint. Port zero and unspecified addresses are rejected.
    ///
    /// # Errors
    /// Returns a network error for an unusable endpoint.
    pub fn new(endpoint: SocketAddr) -> Result<Self, LinuxError> {
        if endpoint.port() == 0 || endpoint.ip().is_unspecified() {
            return Err(LinuxError::new(
                LinuxErrorKind::Network,
                LinuxOperation::Probe,
                LinuxRecovery::CorrectRequest,
                "managed proxy endpoint must name an address and nonzero port",
            ));
        }
        Ok(Self { endpoint })
    }

    /// Returns the exact endpoint.
    #[must_use]
    pub const fn endpoint(self) -> SocketAddr {
        self.endpoint
    }
}

/// Network namespace behavior for one launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkIsolation {
    /// New namespace with no external route.
    DenyAll,
    /// New namespace containing only a helper-bound loopback listener transferred to the parent
    /// managed-proxy owner.
    ManagedProxy,
}

impl NetworkIsolation {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::DenyAll => 0,
            Self::ManagedProxy => 1,
        }
    }
}

/// Exact protected descriptor label for the namespace-local listener transfer channel.
pub const PROXY_LISTENER_LABEL: &str = "linux-netns-proxy-listener";
/// Exact protected descriptor label for the per-launch routing token.
pub const PROXY_TOKEN_LABEL: &str = "linux-netns-proxy-routing-token";

#[cfg(unix)]
pub type ManagedProxyOwner = peritus_network::InheritedListenerProxy;

#[cfg(not(unix))]
#[derive(Debug)]
pub struct ManagedProxyOwner;

#[cfg(unix)]
pub fn shutdown_managed_proxy(proxy: &mut Option<ManagedProxyOwner>) -> Result<(), LinuxError> {
    if let Some(owner) = proxy.take()
        && owner.shutdown().is_err()
    {
        return Err(LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Release,
            LinuxRecovery::Quarantine,
            "managed proxy shutdown did not prove complete worker release",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the cross-platform shutdown contract reports Unix managed-proxy release failures"
)]
pub const fn shutdown_managed_proxy(
    proxy: &mut Option<ManagedProxyOwner>,
) -> Result<(), LinuxError> {
    *proxy = None;
    Ok(())
}
