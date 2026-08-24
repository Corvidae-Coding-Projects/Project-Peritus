//! Deny-dominant requested-name and resolved-address matching.

use std::net::IpAddr;

use peritus_sandbox::{NetworkContract, NetworkDecision, NetworkHost, NetworkTarget, Transport};

use crate::{NetworkError, NetworkPlan};

/// One normalized connection request.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DestinationRequest {
    host: NetworkHost,
    transport: Transport,
    port: u16,
}

impl DestinationRequest {
    /// Creates a nonzero-port destination request.
    ///
    /// # Errors
    /// Rejects port zero and UDP in managed proxy version one.
    pub fn new(host: NetworkHost, transport: Transport, port: u16) -> Result<Self, NetworkError> {
        if port == 0 || transport != Transport::Tcp {
            return Err(crate::error::invalid("managed destination requires nonzero TCP port"));
        }
        Ok(Self { host, transport, port })
    }
    /// Returns the requested host.
    #[must_use]
    pub const fn host(&self) -> &NetworkHost {
        &self.host
    }
    /// Returns transport.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }
    /// Returns port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// Network address risk class used for explicit special-address policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AddressClass {
    /// Globally routable unicast.
    Global,
    /// Loopback.
    Loopback,
    /// Unspecified.
    Unspecified,
    /// Multicast.
    Multicast,
    /// Link-local.
    LinkLocal,
    /// Private/site-local address space.
    Private,
    /// Cloud metadata endpoint.
    Metadata,
}

/// Final request or resolution decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DestinationDecision {
    /// Exact checked authority and runtime policy allow the destination.
    Allowed,
    /// A checked deny rule matched.
    DeniedByRule,
    /// No checked allow rule matched.
    DeniedByDefault,
    /// A special resolved address lacked an exact IP allow rule.
    DeniedSpecialAddress,
}

/// Admitted resolved endpoint bound to its original request.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolvedDestination {
    request: DestinationRequest,
    address: IpAddr,
    class: AddressClass,
}

impl ResolvedDestination {
    /// Returns the original request.
    #[must_use]
    pub const fn request(&self) -> &DestinationRequest {
        &self.request
    }
    /// Returns the selected exact address.
    #[must_use]
    pub const fn address(&self) -> IpAddr {
        self.address
    }
    /// Returns the address class.
    #[must_use]
    pub const fn class(&self) -> AddressClass {
        self.class
    }
}

impl NetworkPlan {
    /// Evaluates the requested host using the checked C2 rules and deny precedence.
    ///
    /// # Errors
    /// Returns an internal validation error only if canonical checked rules cannot be rebuilt.
    pub fn decide_request(
        &self,
        request: &DestinationRequest,
    ) -> Result<DestinationDecision, NetworkError> {
        let contract = NetworkContract::new(self.rules().to_vec()).map_err(|_| {
            NetworkError::new(
                crate::NetworkErrorKind::InvalidInput,
                crate::NetworkOperation::Match,
                crate::RecoveryClass::Replan,
                "checked network rules cannot be reconstructed",
            )
        })?;
        let target = NetworkTarget::new(request.host.clone(), request.transport, request.port)
            .map_err(|_| crate::error::invalid("destination cannot be represented"))?;
        Ok(match contract.decide(&target) {
            NetworkDecision::Allowed => DestinationDecision::Allowed,
            NetworkDecision::DeniedByRule => DestinationDecision::DeniedByRule,
            NetworkDecision::DeniedByDefault => DestinationDecision::DeniedByDefault,
        })
    }

    /// Rechecks a selected DNS answer, including explicit special-address authority.
    ///
    /// # Errors
    /// Returns denial when the requested name was not admitted or the selected address is denied.
    pub fn admit_resolved(
        &self,
        request: &DestinationRequest,
        address: IpAddr,
    ) -> Result<ResolvedDestination, NetworkError> {
        if self.decide_request(request)? != DestinationDecision::Allowed {
            return Err(crate::error::denied("requested network destination is not allowed"));
        }
        let ip_request =
            DestinationRequest::new(NetworkHost::Ip(address), request.transport, request.port)?;
        let ip_decision = self.decide_request(&ip_request)?;
        if ip_decision == DestinationDecision::DeniedByRule {
            return Err(crate::error::denied("resolved address matched an explicit deny rule"));
        }
        let class = classify(address);
        if class != AddressClass::Global && ip_decision != DestinationDecision::Allowed {
            return Err(crate::error::denied("special resolved address lacks an exact IP grant"));
        }
        Ok(ResolvedDestination { request: request.clone(), address, class })
    }
}

/// Classifies addresses before connect.
#[must_use]
pub fn classify(address: IpAddr) -> AddressClass {
    if is_metadata(address) {
        return AddressClass::Metadata;
    }
    if address.is_loopback() {
        return AddressClass::Loopback;
    }
    if address.is_unspecified() {
        return AddressClass::Unspecified;
    }
    if address.is_multicast() {
        return AddressClass::Multicast;
    }
    match address {
        IpAddr::V4(value) if value.is_link_local() => AddressClass::LinkLocal,
        IpAddr::V4(value) if value.is_private() => AddressClass::Private,
        IpAddr::V6(value) if value.is_unicast_link_local() => AddressClass::LinkLocal,
        IpAddr::V6(value) if is_unique_local_v6(value) => AddressClass::Private,
        _ => AddressClass::Global,
    }
}

fn is_metadata(address: IpAddr) -> bool {
    matches!(address, IpAddr::V4(value) if value.octets() == [169, 254, 169, 254])
        || matches!(address, IpAddr::V6(value) if value.octets() == [0xfd,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1])
}

const fn is_unique_local_v6(address: std::net::Ipv6Addr) -> bool {
    address.octets()[0] & 0xfe == 0xfc
}
