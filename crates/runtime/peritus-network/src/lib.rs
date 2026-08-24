//! Verified connection planning and bounded managed outbound proxy.
//!
//! The checked C2 sandbox plan remains authority. This crate projects it into stricter runtime
//! limits, evaluates requested and resolved destinations with deny precedence, and owns bounded
//! proxy sockets and workers. Payloads and credential material are never observation data.

mod accounting;
mod cancellation;
mod canonical;
mod credential;
mod error;
mod matcher;
mod observation;
mod plan;
mod preparation;
mod proxy;
mod recovery;
mod redirect;
mod refinement;
mod resolution;
mod verified;

pub use accounting::{ConnectionAccount, NetworkUsage};
pub use cancellation::CancellationToken;
pub use credential::{
    CredentialLease, CredentialProvider, ProxyCredential, RoutingToken, ScopedCredential,
};
pub use error::{NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass};
pub use matcher::{AddressClass, DestinationDecision, DestinationRequest, ResolvedDestination};
pub use observation::{ConnectionDecision, NetworkObservation, NetworkObservationKind};
pub use plan::{
    DnsMode, NetworkBounds, NetworkPlan, ProxyMode, RedirectMode, RuntimeNetworkOptions,
};
pub use preparation::ManagedProxyPreparation;
#[cfg(unix)]
pub use proxy::{InheritedListenerProxy, send_inherited_listener};
pub use proxy::{ManagedProxy, ProxyEndpoint, ProxyShutdown};
pub use recovery::{ProxyRecoveryRecord, ProxyRecoveryState};
pub use redirect::{RedirectChain, RedirectTarget};
pub use refinement::network_decision_no_broader;
pub use resolution::{Resolver, SystemResolver};
