//! Host for untrusted out-of-process and Wasm Peritus plugins.
//!
//! The host validates declarations and enforces transport/lifecycle resource bounds. A supplied
//! [`AuthorityMediator`] remains the only source of current capability authority.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

mod authority;
mod cancellation;
mod discovery;
mod error;
mod host;
mod quota;
mod transport;
mod trust;

pub use authority::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, HostFuture, InvocationGrant,
    InvocationSubject,
};
pub use cancellation::HostCancellation;
pub use discovery::{DiscoveredPlugin, DiscoveryLimits, PluginCatalog, discover};
pub use error::{HostError, HostFailureClass, RecoveryDisposition};
pub use host::{HostConfig, PluginHost, PluginInvocationResult, PluginLifecycle, PluginSnapshot};
pub use trust::{DigestTrustStore, TrustDecision, TrustVerifier};
