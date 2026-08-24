//! Inert managed-proxy configuration consumed during native preparation.

use core::fmt;
use std::sync::Arc;

use peritus_sandbox::CheckedSandboxPlan;

#[cfg(unix)]
use crate::InheritedListenerProxy;
use crate::{
    ManagedProxy, NetworkError, NetworkPlan, ProxyCredential, Resolver, RoutingToken,
    RuntimeNetworkOptions,
};

/// Network configuration that performs no socket or resolution effect until prepared.
///
/// A platform backend moves this value into the opaque post-consumption callback. Only
/// [`Self::prepare`] compiles the exact checked plan and binds a loopback listener.
pub struct ManagedProxyPreparation {
    options: RuntimeNetworkOptions,
    token: RoutingToken,
    resolver: Arc<dyn Resolver>,
    credential: Option<Arc<ProxyCredential>>,
}

impl ManagedProxyPreparation {
    /// Creates an inert managed-proxy preparation.
    #[must_use]
    pub fn new(
        options: RuntimeNetworkOptions,
        token: RoutingToken,
        resolver: Arc<dyn Resolver>,
        credential: Option<Arc<ProxyCredential>>,
    ) -> Self {
        Self { options, token, resolver, credential }
    }

    /// Compiles the checked plan and starts its one owned loopback proxy.
    ///
    /// # Errors
    ///
    /// Rejects non-narrowing runtime options, credential drift, or listener startup failure.
    pub fn prepare(self, checked: &CheckedSandboxPlan) -> Result<ManagedProxy, NetworkError> {
        let plan = NetworkPlan::from_checked(checked, self.options)?;
        ManagedProxy::start_with(plan, self.token, self.resolver, self.credential)
    }

    /// Compiles the checked plan and starts an owner waiting for a sandbox-local listener.
    ///
    /// The returned Unix channel handle is inherited by the Linux helper. Inside its fresh network
    /// namespace, the helper binds loopback and passes that listener descriptor back with
    /// [`crate::send_inherited_listener`]. The parent proxy accepts through the received socket,
    /// while every upstream connection remains parent-owned outside the target namespace.
    ///
    /// # Errors
    ///
    /// Rejects non-narrowing options or proxy owner/channel startup failure.
    #[cfg(unix)]
    pub fn prepare_inherited_listener(
        self,
        checked: &CheckedSandboxPlan,
    ) -> Result<InheritedListenerProxy, NetworkError> {
        let plan = NetworkPlan::from_checked(checked, self.options)?;
        InheritedListenerProxy::start_with(plan, self.token, self.resolver, self.credential)
    }
}

impl fmt::Debug for ManagedProxyPreparation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedProxyPreparation")
            .field("options", &self.options)
            .field("token", &"[REDACTED]")
            .field("resolver", &"[OPAQUE]")
            .field("credential", &self.credential.as_ref().map(|_| "[SCOPED]"))
            .finish()
    }
}
