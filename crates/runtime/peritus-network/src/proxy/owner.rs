//! Listener lifetime, worker bounds, and complete joins.

use std::{
    net::{Shutdown, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    CancellationToken, ConnectionDecision, NetworkError, NetworkErrorKind, NetworkObservation,
    NetworkObservationKind, NetworkOperation, NetworkPlan, ProxyCredential, RecoveryClass,
    Resolver, RoutingToken,
};

use super::{ProxyShutdown, accept, worker};

pub(super) struct OwnerConfig {
    pub(super) plan: Arc<NetworkPlan>,
    pub(super) token: Arc<RoutingToken>,
    pub(super) resolver: Arc<dyn Resolver>,
    pub(super) credential: Option<Arc<ProxyCredential>>,
    pub(super) cancellation: CancellationToken,
    pub(super) observations: Arc<Mutex<ObservationLog>>,
}

pub(super) struct ObservationLog {
    pub(super) values: Vec<NetworkObservation>,
    maximum: u32,
    next_sequence: u64,
    dropped: u64,
    plan_digest: peritus_types::Sha256Digest,
}

impl ObservationLog {
    pub(super) const fn new(maximum: u32, plan_digest: peritus_types::Sha256Digest) -> Self {
        Self { values: Vec::new(), maximum, next_sequence: 1, dropped: 0, plan_digest }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push(
        &mut self,
        kind: NetworkObservationKind,
        name: Option<peritus_sandbox::DnsName>,
        address: Option<std::net::IpAddr>,
        port: Option<u16>,
        transport: Option<peritus_sandbox::Transport>,
        decision: ConnectionDecision,
        redirect_depth: u8,
        uploaded: u64,
        downloaded: u64,
    ) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        if self.values.len() >= usize::try_from(self.maximum).unwrap_or(usize::MAX) {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.values.push(NetworkObservation::new(
            sequence,
            self.plan_digest,
            kind,
            name,
            address,
            port,
            transport,
            decision,
            redirect_depth,
            uploaded,
            downloaded,
        ));
    }
}

pub(super) struct SharedWorkerConfig {
    pub(super) plan: Arc<NetworkPlan>,
    pub(super) token: Arc<RoutingToken>,
    pub(super) resolver: Arc<dyn Resolver>,
    pub(super) credential: Option<Arc<ProxyCredential>>,
    pub(super) cancellation: CancellationToken,
    pub(super) observations: Arc<Mutex<ObservationLog>>,
    pub(super) total_bytes: Arc<AtomicU64>,
}

pub(super) fn run(
    listener: &TcpListener,
    config: OwnerConfig,
) -> Result<ProxyShutdown, NetworkError> {
    let began = Instant::now();
    let bounds = config.plan.options().bounds();
    let shared = SharedWorkerConfig {
        plan: config.plan,
        token: config.token,
        resolver: config.resolver,
        credential: config.credential,
        cancellation: config.cancellation.clone(),
        observations: config.observations,
        total_bytes: Arc::new(AtomicU64::new(0)),
    };
    let mut accepted = 0_u64;
    let mut workers: Vec<JoinHandle<Result<(), NetworkError>>> = Vec::new();
    let mut workers_joined = true;
    while !shared.cancellation.is_cancelled()
        && u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX) < bounds.total_millis()
    {
        join_finished(&mut workers, &mut workers_joined);
        match accept::next(listener)? {
            Some(mut stream) => {
                accepted = accepted.saturating_add(1);
                if accepted > u64::from(bounds.maximum_connections())
                    || workers.len() >= usize::from(bounds.maximum_workers())
                {
                    let _ = std::io::Write::write_all(
                        &mut stream,
                        b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                    );
                    // Complete the response half of the connection before dropping the
                    // overloaded socket. Windows otherwise resets a socket that still has
                    // unread request bytes, which can discard the 503 response in flight.
                    let _ = stream.shutdown(Shutdown::Write);
                    observe_limited(&shared);
                    continue;
                }
                let worker_config = shared.clone();
                match thread::Builder::new()
                    .name(format!("peritus-network-worker-{accepted}"))
                    .spawn(move || worker::run(stream, &worker_config))
                {
                    Ok(task) => workers.push(task),
                    Err(_) => workers_joined = false,
                }
            }
            None => thread::sleep(Duration::from_millis(5)),
        }
    }
    let _ = shared.cancellation.cancel();
    for task in workers {
        match task.join() {
            Ok(Ok(()) | Err(_)) => {}
            Err(_) => workers_joined = false,
        }
    }
    if let Some(credential) = &shared.credential {
        credential.revoke();
    }
    let mut observations =
        shared.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    observations.push(
        NetworkObservationKind::Released,
        None,
        None,
        None,
        None,
        if workers_joined { ConnectionDecision::Allowed } else { ConnectionDecision::Failed },
        0,
        0,
        0,
    );
    let retained = u32::try_from(observations.values.len()).unwrap_or(u32::MAX);
    let dropped = observations.dropped;
    drop(observations);
    Ok(ProxyShutdown {
        accepted_connections: accepted,
        workers_joined,
        retained_observations: retained,
        dropped_observations: dropped,
    })
}

impl Clone for SharedWorkerConfig {
    fn clone(&self) -> Self {
        Self {
            plan: Arc::clone(&self.plan),
            token: Arc::clone(&self.token),
            resolver: Arc::clone(&self.resolver),
            credential: self.credential.as_ref().map(Arc::clone),
            cancellation: self.cancellation.clone(),
            observations: Arc::clone(&self.observations),
            total_bytes: Arc::clone(&self.total_bytes),
        }
    }
}

fn join_finished(workers: &mut Vec<JoinHandle<Result<(), NetworkError>>>, joined: &mut bool) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let task = workers.swap_remove(index);
            if task.join().is_err() {
                *joined = false;
            }
        } else {
            index += 1;
        }
    }
}

fn observe_limited(config: &SharedWorkerConfig) {
    config.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(
        NetworkObservationKind::Closed,
        None,
        None,
        None,
        None,
        ConnectionDecision::Limited,
        0,
        0,
        0,
    );
}

pub(super) fn charge_total(total: &AtomicU64, charge: u64, limit: u64) -> Result<(), NetworkError> {
    total
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(charge).filter(|next| *next <= limit)
        })
        .map(|_| ())
        .map_err(|_| {
            NetworkError::new(
                NetworkErrorKind::Limit,
                NetworkOperation::Relay,
                RecoveryClass::CancelAndJoin,
                "aggregate proxy byte ceiling was crossed",
            )
        })
}

pub(super) const fn proxy_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Proxy,
        NetworkOperation::Proxy,
        RecoveryClass::Retry,
        detail,
    )
}

pub(super) const fn teardown_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::IncompleteTeardown,
        NetworkOperation::Shutdown,
        RecoveryClass::CancelAndJoin,
        detail,
    )
}
