//! Owned loopback HTTP/CONNECT proxy.

mod accept;
mod connect;
mod http;
#[cfg(unix)]
mod inherited;
mod owner;
mod redirect_worker;
mod storage;
mod worker;

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use crate::{
    CancellationToken, NetworkError, NetworkObservation, NetworkPlan, ProxyCredential, Resolver,
    RoutingToken, SystemResolver,
};

#[cfg(unix)]
pub use inherited::{InheritedListenerProxy, send_inherited_listener};

/// Loopback endpoint exposed only to the sandboxed launch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProxyEndpoint(SocketAddr);

impl ProxyEndpoint {
    /// Returns the loopback socket address.
    #[must_use]
    pub const fn socket_addr(self) -> SocketAddr {
        self.0
    }
}

/// Terminal proxy-owner accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProxyShutdown {
    accepted_connections: u64,
    workers_joined: bool,
    retained_observations: u64,
    dropped_observations: u64,
}

/// Stable terminal receipt from reconciling the exact retained proxy owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProxyReconciliation {
    shutdown: Option<ProxyShutdown>,
    terminal_failure: Option<NetworkError>,
}

impl ProxyReconciliation {
    /// Returns normal terminal accounting when the owner completed without a terminal failure.
    #[must_use]
    pub const fn shutdown(self) -> Option<ProxyShutdown> {
        self.shutdown
    }

    /// Returns the owner's terminal failure after its listener and workers were proven absent.
    #[must_use]
    pub const fn terminal_failure(self) -> Option<NetworkError> {
        self.terminal_failure
    }
}

enum ProxyOwnerState {
    Running(JoinHandle<Result<ProxyShutdown, NetworkError>>),
    Reconciled(ProxyReconciliation),
    ReconciliationRequired(NetworkError),
}

impl ProxyShutdown {
    /// Returns accepted connections.
    #[must_use]
    pub const fn accepted_connections(self) -> u64 {
        self.accepted_connections
    }
    /// Returns whether every worker joined.
    #[must_use]
    pub const fn workers_joined(self) -> bool {
        self.workers_joined
    }
    /// Returns retained observations.
    #[must_use]
    pub const fn retained_observations(self) -> u64 {
        self.retained_observations
    }
    /// Returns observations dropped without retention.
    ///
    /// The current owner never drops an observation; a selected ceiling terminates with a typed
    /// limit failure instead.
    #[must_use]
    pub const fn dropped_observations(self) -> u64 {
        self.dropped_observations
    }
}

/// Owner of one loopback listener and every accepted worker task.
#[must_use = "the managed proxy must be shut down or dropped to cancel and join workers"]
pub struct ManagedProxy {
    endpoint: ProxyEndpoint,
    plan_digest: peritus_types::Sha256Digest,
    token: Arc<RoutingToken>,
    cancellation: CancellationToken,
    observations: Arc<Mutex<owner::ObservationLog>>,
    owner: ProxyOwnerState,
}

impl ManagedProxy {
    /// Starts a proxy using the system resolver and no upstream credential injection.
    ///
    /// # Errors
    /// Returns a typed error when the listener, observation storage, or owner cannot be created.
    pub fn start(plan: NetworkPlan, token: RoutingToken) -> Result<Self, NetworkError> {
        Self::start_with(plan, token, Arc::new(SystemResolver), None)
    }

    /// Starts a proxy with explicit resolver and optional exact credential lease.
    ///
    /// # Errors
    /// Returns a typed error when the listener, observation storage, or owner cannot be created.
    pub fn start_with(
        plan: NetworkPlan,
        token: RoutingToken,
        resolver: Arc<dyn Resolver>,
        credential: Option<Arc<ProxyCredential>>,
    ) -> Result<Self, NetworkError> {
        let plan_digest = plan.digest();
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .map_err(|_| owner::proxy_error("managed proxy cannot bind loopback"))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| owner::proxy_error("managed proxy listener cannot be nonblocking"))?;
        let endpoint = ProxyEndpoint(
            listener
                .local_addr()
                .map_err(|_| owner::proxy_error("managed proxy address is unavailable"))?,
        );
        let token = Arc::new(token);
        let cancellation = CancellationToken::new();
        let observations = Arc::new(Mutex::new(owner::ObservationLog::new(
            plan.options().bounds().observations(),
            plan.digest(),
        )?));
        let config = owner::OwnerConfig {
            plan: Arc::new(plan),
            token: Arc::clone(&token),
            resolver,
            credential,
            cancellation: cancellation.clone(),
            observations: Arc::clone(&observations),
        };
        let join = std::thread::Builder::new()
            .name("peritus-network-proxy".to_owned())
            .spawn(move || owner::run(&listener, config))
            .map_err(|_| owner::proxy_error("managed proxy owner thread cannot be started"))?;
        Ok(Self {
            endpoint,
            plan_digest,
            token,
            cancellation,
            observations,
            owner: ProxyOwnerState::Running(join),
        })
    }

    /// Returns the loopback endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> ProxyEndpoint {
        self.endpoint
    }

    /// Borrows the opaque routing token for exact child configuration.
    #[must_use]
    pub fn routing_token(&self) -> &RoutingToken {
        &self.token
    }

    /// Returns a stable identity while the exact owner or its unresolved join receipt is retained.
    #[must_use]
    pub fn custody_identity(&self) -> Option<peritus_types::Sha256Digest> {
        if matches!(&self.owner, ProxyOwnerState::Reconciled(_)) {
            return None;
        }
        let mut bytes = Vec::from(b"PERITUS-MANAGED-PROXY-OWNER-V1\0".as_slice());
        bytes.extend_from_slice(self.plan_digest.as_bytes());
        match self.endpoint.socket_addr() {
            SocketAddr::V4(address) => {
                bytes.push(4);
                bytes.extend_from_slice(&address.ip().octets());
                bytes.extend_from_slice(&address.port().to_be_bytes());
            }
            SocketAddr::V6(address) => {
                bytes.push(6);
                bytes.extend_from_slice(&address.ip().octets());
                bytes.extend_from_slice(&address.port().to_be_bytes());
                bytes.extend_from_slice(&address.flowinfo().to_be_bytes());
                bytes.extend_from_slice(&address.scope_id().to_be_bytes());
            }
        }
        self.token
            .expose_bytes(|token| bytes.extend_from_slice(token));
        Some(peritus_codec::sha256(&bytes))
    }

    /// Returns a compatibility snapshot of retained normalized observations.
    ///
    /// Production consumers should use [`Self::observation_page`] so each transfer remains
    /// physically bounded. A storage fault makes this compatibility snapshot empty and remains
    /// latched for [`Self::shutdown`].
    #[must_use]
    pub fn observations(&self) -> Vec<NetworkObservation> {
        self.try_observations().unwrap_or_default()
    }

    /// Returns a fallible compatibility snapshot of the complete retained history.
    ///
    /// # Errors
    /// Returns a latched storage or selected-ceiling failure.
    pub fn try_observations(&self) -> Result<Vec<NetworkObservation>, NetworkError> {
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
    }

    /// Returns one bounded physical page after `after_sequence`.
    ///
    /// Pass zero for the first page and then the returned `next_sequence`. The page size does not
    /// limit retained history.
    ///
    /// # Errors
    /// Rejects an invalid cursor or an unreadable committed record. A later append failure remains
    /// latched for shutdown without hiding the already committed prefix.
    pub fn observation_page(
        &self,
        after_sequence: u64,
    ) -> Result<crate::NetworkObservationPage, NetworkError> {
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .page(after_sequence)
    }

    /// Cancels the listener and joins every owned worker.
    ///
    /// # Errors
    /// Returns a retained owner, storage, policy, panic, or incomplete-join failure.
    pub fn shutdown(mut self) -> Result<ProxyShutdown, NetworkError> {
        let receipt = self.reconcile_shutdown()?;
        if let Some(error) = receipt.terminal_failure() {
            return Err(error);
        }
        receipt
            .shutdown()
            .ok_or_else(|| owner::reconciliation_error("proxy shutdown receipt is incomplete"))
    }

    /// Cancels the exact retained owner and reconciles its terminal join evidence.
    ///
    /// Unlike [`Self::shutdown`], this keeps the owner object available to the caller when the
    /// owner reports a terminal policy, storage, or worker failure. A successful thread join
    /// proves the listener and every owned worker have terminated even when their terminal result
    /// is an error. The returned receipt retains that terminal cause while repeated calls return
    /// the same absence proof without starting or substituting another proxy.
    ///
    /// # Errors
    /// Returns an incomplete teardown failure when the owner thread itself panicked and absence
    /// cannot be proved. That exact reconciliation requirement is stable across repeated calls.
    pub fn reconcile_shutdown(&mut self) -> Result<ProxyReconciliation, NetworkError> {
        let _ = self.cancellation.cancel();
        match &self.owner {
            ProxyOwnerState::Reconciled(receipt) => return Ok(*receipt),
            ProxyOwnerState::ReconciliationRequired(error) => return Err(*error),
            ProxyOwnerState::Running(_) => {}
        }
        let placeholder = owner::reconciliation_error("proxy owner join remains indeterminate");
        let ProxyOwnerState::Running(join) =
            core::mem::replace(&mut self.owner, ProxyOwnerState::ReconciliationRequired(placeholder))
        else {
            unreachable!("running proxy state was checked before join extraction");
        };
        let next = match join.join() {
            Ok(Ok(shutdown)) if shutdown.workers_joined() => {
                ProxyOwnerState::Reconciled(ProxyReconciliation {
                    shutdown: Some(shutdown),
                    terminal_failure: None,
                })
            }
            Ok(Ok(_)) => ProxyOwnerState::ReconciliationRequired(owner::reconciliation_error(
                "proxy owner returned incomplete worker accounting",
            )),
            Ok(Err(error)) => ProxyOwnerState::Reconciled(ProxyReconciliation {
                shutdown: None,
                terminal_failure: Some(error),
            }),
            Err(_) => ProxyOwnerState::ReconciliationRequired(owner::reconciliation_error(
                "proxy owner thread panicked before absence was proved",
            )),
        };
        self.owner = next;
        match &self.owner {
            ProxyOwnerState::Reconciled(receipt) => Ok(*receipt),
            ProxyOwnerState::ReconciliationRequired(error) => Err(*error),
            ProxyOwnerState::Running(_) => unreachable!("proxy reconciliation produced running state"),
        }
    }
}

impl fmt::Debug for ManagedProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedProxy")
            .field("endpoint", &self.endpoint)
            .field("routing_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl Drop for ManagedProxy {
    fn drop(&mut self) {
        if matches!(&self.owner, ProxyOwnerState::Running(_)) {
            let _ = self.reconcile_shutdown();
        }
    }
}
