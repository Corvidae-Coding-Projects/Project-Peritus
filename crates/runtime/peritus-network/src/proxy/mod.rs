//! Bounded loopback HTTP/CONNECT proxy owner.

mod accept;
mod connect;
mod http;
#[cfg(unix)]
mod inherited;
mod owner;
mod redirect_worker;
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
    retained_observations: u32,
    dropped_observations: u64,
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
    pub const fn retained_observations(self) -> u32 {
        self.retained_observations
    }
    /// Returns observations dropped at the configured ceiling.
    #[must_use]
    pub const fn dropped_observations(self) -> u64 {
        self.dropped_observations
    }
}

/// Owner of one loopback listener and all bounded worker tasks.
#[must_use = "the managed proxy must be shut down or dropped to cancel and join workers"]
pub struct ManagedProxy {
    endpoint: ProxyEndpoint,
    token: Arc<RoutingToken>,
    cancellation: CancellationToken,
    observations: Arc<Mutex<owner::ObservationLog>>,
    join: Option<JoinHandle<Result<ProxyShutdown, NetworkError>>>,
}

impl ManagedProxy {
    /// Starts a proxy using the system resolver and no upstream credential injection.
    ///
    /// # Errors
    /// Returns a typed error when the loopback listener or owner thread cannot be created.
    pub fn start(plan: NetworkPlan, token: RoutingToken) -> Result<Self, NetworkError> {
        Self::start_with(plan, token, Arc::new(SystemResolver), None)
    }

    /// Starts a proxy with explicit resolver and optional exact credential lease.
    ///
    /// # Errors
    /// Returns a typed error when the loopback listener or owner thread cannot be created.
    pub fn start_with(
        plan: NetworkPlan,
        token: RoutingToken,
        resolver: Arc<dyn Resolver>,
        credential: Option<Arc<ProxyCredential>>,
    ) -> Result<Self, NetworkError> {
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
        )));
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
        Ok(Self { endpoint, token, cancellation, observations, join: Some(join) })
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

    /// Returns a snapshot of retained normalized observations.
    #[must_use]
    pub fn observations(&self) -> Vec<NetworkObservation> {
        self.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).values.clone()
    }

    /// Cancels the listener and joins every owned worker.
    ///
    /// # Errors
    /// Returns a typed failure if the owner panicked or could not establish complete joins.
    pub fn shutdown(mut self) -> Result<ProxyShutdown, NetworkError> {
        self.join_owner()
    }

    fn join_owner(&mut self) -> Result<ProxyShutdown, NetworkError> {
        let _ = self.cancellation.cancel();
        let join = self
            .join
            .take()
            .ok_or_else(|| owner::teardown_error("proxy owner was already joined"))?;
        let result =
            join.join().map_err(|_| owner::teardown_error("proxy owner thread panicked"))??;
        if !result.workers_joined() {
            return Err(owner::teardown_error("proxy worker teardown was incomplete"));
        }
        Ok(result)
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
        if self.join.is_some() {
            let _ = self.join_owner();
        }
    }
}
