//! Listener lifetime, worker bounds, and complete joins.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, TcpListener},
    num::NonZeroU64,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    CancellationToken, ConnectionAccount, ConnectionDecision, NetworkError, NetworkErrorKind,
    NetworkObservation, NetworkObservationKind, NetworkOperation, NetworkPlan, ProxyCredential,
    RecoveryClass, Resolver, RoutingToken,
};

use peritus_sandbox::{DnsName, Transport};

use super::{ProxyShutdown, accept, storage::TemporaryFile, worker};

const PHYSICAL_WORKER_WINDOW: usize = 64;
const OBSERVATION_MAGIC: &[u8; 8] = b"PNOBS001";
const OBSERVATION_NAME_BYTES: usize = 253;
const OBSERVATION_RECORD_BYTES: usize = 374;
const OBSERVATION_CHECKSUM_OFFSET: usize = OBSERVATION_RECORD_BYTES - 32;

pub(super) struct OwnerConfig {
    pub(super) plan: Arc<NetworkPlan>,
    pub(super) token: Arc<RoutingToken>,
    pub(super) resolver: Arc<dyn Resolver>,
    pub(super) credential: Option<Arc<ProxyCredential>>,
    pub(super) cancellation: CancellationToken,
    pub(super) observations: Arc<Mutex<ObservationLog>>,
}

pub(super) struct ObservationLog {
    file: TemporaryFile,
    maximum: Option<NonZeroU64>,
    count: u64,
    next_sequence: u64,
    plan_digest: peritus_types::Sha256Digest,
    failure: Option<ObservationFailure>,
}

#[derive(Clone, Copy)]
enum ObservationFailure {
    Limit,
    Storage,
    LimitThenStorage,
}

impl ObservationLog {
    pub(super) fn new(
        maximum: Option<NonZeroU64>,
        plan_digest: peritus_types::Sha256Digest,
    ) -> Result<Self, NetworkError> {
        let file = TemporaryFile::create("observations").map_err(|_| observation_storage_error())?;
        Ok(Self {
            file,
            maximum,
            count: 0,
            next_sequence: 1,
            plan_digest,
            failure: None,
        })
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
    ) -> Result<(), NetworkError> {
        self.ensure_healthy()?;
        if matches!(self.maximum, Some(maximum) if self.count >= maximum.get()) {
            return self.fail(ObservationFailure::Limit);
        }
        let sequence = self.next_sequence;
        let Some(next_sequence) = self.next_sequence.checked_add(1) else {
            return self.fail(ObservationFailure::Limit);
        };
        let Some(next_count) = self.count.checked_add(1) else {
            return self.fail(ObservationFailure::Limit);
        };
        let observation = NetworkObservation::new(
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
        );
        let Some(record) = encode_observation(&observation) else {
            return self.fail(ObservationFailure::Storage);
        };
        let Ok(record_bytes) = u64::try_from(OBSERVATION_RECORD_BYTES) else {
            return self.fail(ObservationFailure::Storage);
        };
        let Some(offset) = self.count.checked_mul(record_bytes) else {
            return self.fail(ObservationFailure::Limit);
        };
        if self.file.write_all_at(offset, &record).is_err() || self.file.sync_data().is_err() {
            return self.fail(ObservationFailure::Storage);
        }
        self.next_sequence = next_sequence;
        self.count = next_count;
        Ok(())
    }

    pub(super) fn values(&mut self) -> Result<Vec<NetworkObservation>, NetworkError> {
        self.ensure_healthy()?;
        let mut values = Vec::new();
        let mut cursor = 0_u64;
        while cursor < self.count {
            let page = self.page(cursor)?;
            if values.try_reserve(page.observations().len()).is_err() {
                return self.fail(ObservationFailure::Storage);
            }
            values.extend_from_slice(page.observations());
            let next = page.next_sequence();
            if next <= cursor {
                return self.fail(ObservationFailure::Storage);
            }
            cursor = next;
        }
        Ok(values)
    }

    pub(super) fn page(
        &mut self,
        after_sequence: u64,
    ) -> Result<crate::NetworkObservationPage, NetworkError> {
        if after_sequence > self.count {
            return Err(crate::error::invalid(
                "network observation cursor is beyond retained history",
            ));
        }
        let mut observations = Vec::new();
        if observations.try_reserve_exact(crate::OBSERVATION_PAGE_RECORDS).is_err() {
            return self.fail(ObservationFailure::Storage);
        }
        let remaining = self.count - after_sequence;
        let Ok(records_per_page) = u64::try_from(crate::OBSERVATION_PAGE_RECORDS) else {
            return self.fail(ObservationFailure::Storage);
        };
        let Ok(record_bytes) = u64::try_from(OBSERVATION_RECORD_BYTES) else {
            return self.fail(ObservationFailure::Storage);
        };
        let page_records = remaining.min(records_per_page);
        for relative in 0..page_records {
            let Some(index) = after_sequence.checked_add(relative) else {
                return self.fail(ObservationFailure::Storage);
            };
            let Some(offset) = index.checked_mul(record_bytes) else {
                return self.fail(ObservationFailure::Storage);
            };
            let mut record = [0_u8; OBSERVATION_RECORD_BYTES];
            if self.file.read_exact_at(offset, &mut record).is_err() {
                return self.fail(ObservationFailure::Storage);
            }
            let Some(expected_sequence) = index.checked_add(1) else {
                return self.fail(ObservationFailure::Storage);
            };
            let observation = match decode_observation(
                &record,
                expected_sequence,
                self.plan_digest,
            ) {
                Some(observation) => observation,
                None => return self.fail(ObservationFailure::Storage),
            };
            observations.push(observation);
        }
        let consumed = u64::try_from(observations.len()).map_err(|_| {
            crate::error::invalid("network observation page length is not representable")
        })?;
        let next_sequence = after_sequence.checked_add(consumed).ok_or_else(|| {
            crate::error::invalid("network observation cursor overflowed")
        })?;
        Ok(crate::NetworkObservationPage::new(
            observations,
            next_sequence,
            next_sequence == self.count,
        ))
    }

    pub(super) const fn len(&self) -> u64 {
        self.count
    }

    pub(super) fn failure(&self) -> Result<(), NetworkError> {
        match self.failure {
            Some(failure) => Err(failure.error()),
            None => Ok(()),
        }
    }

    fn ensure_healthy(&self) -> Result<(), NetworkError> {
        self.failure()
    }

    fn fail<T>(&mut self, failure: ObservationFailure) -> Result<T, NetworkError> {
        let retained = match (self.failure, failure) {
            (None, failure) => failure,
            (
                Some(ObservationFailure::Limit),
                ObservationFailure::Storage | ObservationFailure::LimitThenStorage,
            ) => {
                ObservationFailure::LimitThenStorage
            }
            (Some(ObservationFailure::Limit), ObservationFailure::Limit) => {
                ObservationFailure::Limit
            }
            (Some(ObservationFailure::Storage), _) => ObservationFailure::Storage,
            (Some(ObservationFailure::LimitThenStorage), _) => {
                ObservationFailure::LimitThenStorage
            }
        };
        self.failure = Some(retained);
        Err(retained.error())
    }
}

impl ObservationFailure {
    const fn error(self) -> NetworkError {
        match self {
            Self::Limit => observation_limit_error(),
            Self::Storage => observation_storage_error(),
            Self::LimitThenStorage => observation_storage_after_limit_error(),
        }
    }
}

fn encode_observation(
    observation: &NetworkObservation,
) -> Option<[u8; OBSERVATION_RECORD_BYTES]> {
    let mut record = [0_u8; OBSERVATION_RECORD_BYTES];
    record[..8].copy_from_slice(OBSERVATION_MAGIC);
    record[8..16].copy_from_slice(&observation.sequence().to_be_bytes());
    record[16..48].copy_from_slice(observation.plan_digest().as_bytes());
    record[48] = observation_kind_tag(observation.kind());
    record[49] = decision_tag(observation.decision());
    record[50] = observation.redirect_depth();
    if let Some(name) = observation.requested_name() {
        let bytes = name.as_str().as_bytes();
        if bytes.len() > OBSERVATION_NAME_BYTES {
            return None;
        }
        let length = u16::try_from(bytes.len()).ok()?;
        record[51..53].copy_from_slice(&length.to_be_bytes());
        record[73..73 + bytes.len()].copy_from_slice(bytes);
    }
    match observation.selected_address() {
        Some(IpAddr::V4(address)) => {
            record[53] = 4;
            record[326..330].copy_from_slice(&address.octets());
        }
        Some(IpAddr::V6(address)) => {
            record[53] = 6;
            record[326..342].copy_from_slice(&address.octets());
        }
        None => {}
    }
    record[54] = observation.transport().map_or(u8::MAX, transport_tag);
    record[55..57].copy_from_slice(&observation.port().unwrap_or(0).to_be_bytes());
    record[57..65].copy_from_slice(&observation.uploaded().to_be_bytes());
    record[65..73].copy_from_slice(&observation.downloaded().to_be_bytes());
    let checksum = peritus_codec::sha256(&record[..OBSERVATION_CHECKSUM_OFFSET]);
    record[OBSERVATION_CHECKSUM_OFFSET..].copy_from_slice(checksum.as_bytes());
    Some(record)
}

fn decode_observation(
    record: &[u8; OBSERVATION_RECORD_BYTES],
    expected_sequence: u64,
    expected_plan: peritus_types::Sha256Digest,
) -> Option<NetworkObservation> {
    if &record[..8] != OBSERVATION_MAGIC {
        return None;
    }
    let checksum = peritus_codec::sha256(&record[..OBSERVATION_CHECKSUM_OFFSET]);
    if &record[OBSERVATION_CHECKSUM_OFFSET..] != checksum.as_bytes() {
        return None;
    }
    let sequence = u64::from_be_bytes(record[8..16].try_into().ok()?);
    let plan_digest = peritus_types::Sha256Digest::new(record[16..48].try_into().ok()?);
    if sequence != expected_sequence || plan_digest != expected_plan {
        return None;
    }
    let kind = observation_kind(record[48])?;
    let decision = decision(record[49])?;
    let redirect_depth = record[50];
    let name_length = usize::from(u16::from_be_bytes(record[51..53].try_into().ok()?));
    if name_length > OBSERVATION_NAME_BYTES {
        return None;
    }
    let requested_name = if name_length == 0 {
        None
    } else {
        let name = std::str::from_utf8(&record[73..73 + name_length]).ok()?;
        Some(DnsName::new(name.to_owned()).ok()?)
    };
    let selected_address = match record[53] {
        0 => None,
        4 => Some(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(&record[326..330]).ok()?))),
        6 => Some(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&record[326..342]).ok()?))),
        _ => return None,
    };
    let transport = match record[54] {
        u8::MAX => None,
        value => Some(transport(value)?),
    };
    let port = match u16::from_be_bytes(record[55..57].try_into().ok()?) {
        0 => None,
        value => Some(value),
    };
    let uploaded = u64::from_be_bytes(record[57..65].try_into().ok()?);
    let downloaded = u64::from_be_bytes(record[65..73].try_into().ok()?);
    Some(NetworkObservation::new(
        sequence,
        plan_digest,
        kind,
        requested_name,
        selected_address,
        port,
        transport,
        decision,
        redirect_depth,
        uploaded,
        downloaded,
    ))
}

const fn observation_kind_tag(kind: NetworkObservationKind) -> u8 {
    match kind {
        NetworkObservationKind::Requested => 0,
        NetworkObservationKind::Resolved => 1,
        NetworkObservationKind::Connected => 2,
        NetworkObservationKind::Redirected => 3,
        NetworkObservationKind::CredentialInjected => 4,
        NetworkObservationKind::Closed => 5,
        NetworkObservationKind::Released => 6,
    }
}

const fn observation_kind(tag: u8) -> Option<NetworkObservationKind> {
    match tag {
        0 => Some(NetworkObservationKind::Requested),
        1 => Some(NetworkObservationKind::Resolved),
        2 => Some(NetworkObservationKind::Connected),
        3 => Some(NetworkObservationKind::Redirected),
        4 => Some(NetworkObservationKind::CredentialInjected),
        5 => Some(NetworkObservationKind::Closed),
        6 => Some(NetworkObservationKind::Released),
        _ => None,
    }
}

const fn decision_tag(decision: ConnectionDecision) -> u8 {
    match decision {
        ConnectionDecision::Allowed => 0,
        ConnectionDecision::Denied => 1,
        ConnectionDecision::Failed => 2,
        ConnectionDecision::Limited => 3,
        ConnectionDecision::Cancelled => 4,
    }
}

const fn decision(tag: u8) -> Option<ConnectionDecision> {
    match tag {
        0 => Some(ConnectionDecision::Allowed),
        1 => Some(ConnectionDecision::Denied),
        2 => Some(ConnectionDecision::Failed),
        3 => Some(ConnectionDecision::Limited),
        4 => Some(ConnectionDecision::Cancelled),
        _ => None,
    }
}

const fn transport_tag(transport: Transport) -> u8 {
    match transport {
        Transport::Tcp => 0,
        Transport::Udp => 1,
    }
}

const fn transport(tag: u8) -> Option<Transport> {
    match tag {
        0 => Some(Transport::Tcp),
        1 => Some(Transport::Udp),
        _ => None,
    }
}

pub(super) struct SharedWorkerConfig {
    pub(super) plan: Arc<NetworkPlan>,
    pub(super) token: Arc<RoutingToken>,
    pub(super) resolver: Arc<dyn Resolver>,
    pub(super) credential: Option<Arc<ProxyCredential>>,
    pub(super) cancellation: CancellationToken,
    pub(super) observations: Arc<Mutex<ObservationLog>>,
    pub(super) total_bytes: Arc<Mutex<u64>>,
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
        total_bytes: Arc::new(Mutex::new(0)),
    };
    let mut accepted = 0_u64;
    let mut workers: Vec<JoinHandle<Result<(), NetworkError>>> = Vec::new();
    let mut workers_joined = true;
    let mut owner_failure = None;
    while !shared.cancellation.is_cancelled() {
        if let Some(limit) = bounds.total_millis() {
            match elapsed_millis(began) {
                Ok(elapsed) if elapsed >= limit.get() => break,
                Ok(_) => {}
                Err(error) => {
                    owner_failure = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = join_finished(&mut workers, &mut workers_joined) {
            owner_failure = Some(error);
            break;
        }
        if let Err(error) = shared
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .failure()
        {
            owner_failure = Some(error);
            break;
        }
        if workers.len() >= PHYSICAL_WORKER_WINDOW {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        let accepted_stream = match accept::next(listener) {
            Ok(value) => value,
            Err(error) => {
                owner_failure = Some(error);
                break;
            }
        };
        match accepted_stream {
            Some(mut stream) => {
                let Some(next_accepted) = accepted.checked_add(1) else {
                    owner_failure = Some(connection_limit_error());
                    break;
                };
                accepted = next_accepted;
                let connections_limited = bounds
                    .maximum_connections()
                    .is_some_and(|maximum| accepted > maximum.get());
                let workers_limited = bounds.maximum_workers().is_some_and(|maximum| {
                    u64::try_from(workers.len()).is_ok_and(|active| active >= maximum.get())
                });
                if connections_limited || workers_limited
                {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                    let _ = std::io::Write::write_all(
                        &mut stream,
                        b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                    );
                    // Complete the response half of the connection before dropping the
                    // overloaded socket. Windows otherwise resets a socket that still has
                    // unread request bytes, which can discard the 503 response in flight.
                    let _ = stream.shutdown(Shutdown::Write);
                    if let Err(error) = observe_limited(&shared) {
                        owner_failure = Some(error);
                        break;
                    }
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
            Ok(Err(error)) if error.is_storage() => {
                owner_failure = Some(error);
            }
            Ok(Ok(()) | Err(_)) => {}
            Err(_) => workers_joined = false,
        }
    }
    if let Some(credential) = &shared.credential {
        credential.revoke();
    }
    let mut observations =
        shared.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let release_failure = observations
        .push(
            NetworkObservationKind::Released,
            None,
            None,
            None,
            None,
            if workers_joined && owner_failure.is_none() {
                ConnectionDecision::Allowed
            } else {
                ConnectionDecision::Failed
            },
            0,
            0,
            0,
        )
        .err();
    let retained = observations.len();
    let observation_failure = observations.failure().err();
    drop(observations);
    if let Some(error) = owner_failure
        && error.is_storage()
    {
        return Err(error);
    }
    if let Some(error) = release_failure
        && error.is_storage()
    {
        return Err(error);
    }
    if let Some(error) = observation_failure
        && error.is_storage()
    {
        return Err(error);
    }
    if !workers_joined {
        return Err(teardown_error("proxy worker teardown was incomplete"));
    }
    if let Some(error) = release_failure {
        return Err(error);
    }
    if let Some(error) = observation_failure {
        return Err(error);
    }
    if let Some(error) = owner_failure {
        return Err(error);
    }
    Ok(ProxyShutdown {
        accepted_connections: accepted,
        workers_joined,
        retained_observations: retained,
        dropped_observations: 0,
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

fn join_finished(
    workers: &mut Vec<JoinHandle<Result<(), NetworkError>>>,
    joined: &mut bool,
) -> Option<NetworkError> {
    let mut storage_failure = None;
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let task = workers.swap_remove(index);
            match task.join() {
                Ok(Err(error)) if error.is_storage() => {
                    storage_failure.get_or_insert(error);
                }
                Ok(Ok(()) | Err(_)) => {}
                Err(_) => *joined = false,
            }
        } else {
            index += 1;
        }
    }
    storage_failure
}

fn observe_limited(config: &SharedWorkerConfig) -> Result<(), NetworkError> {
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
    )
}

pub(super) fn charge(
    account: &mut ConnectionAccount,
    total: &Mutex<u64>,
    charge: u64,
    limit: Option<NonZeroU64>,
    upload: bool,
) -> Result<(), NetworkError> {
    let mut next_account = *account;
    if upload {
        next_account.charge_upload(charge)?;
    } else {
        next_account.charge_download(charge)?;
    }
    let mut total = total.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let next_total = total.checked_add(charge).ok_or_else(aggregate_limit_error)?;
    if limit.is_some_and(|limit| next_total > limit.get()) {
        return Err(aggregate_limit_error());
    }
    *total = next_total;
    *account = next_account;
    Ok(())
}

fn elapsed_millis(began: Instant) -> Result<u64, NetworkError> {
    u64::try_from(began.elapsed().as_millis()).map_err(|_| connection_limit_error())
}

const fn observation_limit_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Proxy,
        RecoveryClass::CancelAndJoin,
        "selected managed-network observation ceiling was crossed",
    )
}

const fn observation_storage_error() -> NetworkError {
    NetworkError::storage(
        NetworkOperation::Proxy,
        "managed-network observation storage cannot be created, written, or read",
    )
}

const fn observation_storage_after_limit_error() -> NetworkError {
    NetworkError::storage_after_limit(
        NetworkOperation::Proxy,
        "selected observation ceiling was crossed before observation storage also failed",
    )
}

const fn connection_limit_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Proxy,
        RecoveryClass::CancelAndJoin,
        "managed-network connection accounting is not representable",
    )
}

const fn aggregate_limit_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "aggregate proxy byte accounting overflowed or crossed its selected ceiling",
    )
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

pub(super) const fn reconciliation_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::IncompleteTeardown,
        NetworkOperation::Shutdown,
        RecoveryClass::Reconcile,
        detail,
    )
}
