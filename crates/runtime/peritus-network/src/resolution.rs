//! Per-connection DNS resolution with exact answer revalidation.

use std::{
    collections::BTreeSet,
    net::{IpAddr, ToSocketAddrs},
};

use peritus_sandbox::NetworkHost;

use crate::{DestinationRequest, NetworkError, ResolvedDestination};

/// Resolver abstraction used by deterministic and system-backed proxy tests.
pub trait Resolver: Send + Sync + 'static {
    /// Resolves and revalidates every returned address.
    ///
    /// # Errors
    /// Fails the complete resolution if any answer is denied, empty, or cannot be resolved.
    fn resolve(
        &self,
        plan: &crate::NetworkPlan,
        request: &DestinationRequest,
    ) -> Result<Vec<ResolvedDestination>, NetworkError>;
}

/// System resolver used by the managed proxy.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(
        &self,
        plan: &crate::NetworkPlan,
        request: &DestinationRequest,
    ) -> Result<Vec<ResolvedDestination>, NetworkError> {
        let addresses: BTreeSet<IpAddr> = match request.host() {
            NetworkHost::Ip(address) => std::iter::once(*address).collect(),
            NetworkHost::Dns(name) => (name.as_str(), request.port())
                .to_socket_addrs()
                .map_err(|_| resolution_error("DNS name could not be resolved"))?
                .map(|socket| socket.ip())
                .collect(),
        };
        if addresses.is_empty() {
            return Err(resolution_error("DNS resolution returned no addresses"));
        }
        addresses.into_iter().map(|address| plan.admit_resolved(request, address)).collect()
    }
}

const fn resolution_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        crate::NetworkErrorKind::Resolution,
        crate::NetworkOperation::Resolve,
        crate::RecoveryClass::Retry,
        detail,
    )
}
