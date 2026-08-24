//! One authenticated, plan-bound proxy connection.

use std::{
    io::Write,
    net::TcpStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use peritus_sandbox::{DnsName, NetworkHost};

use crate::{
    ConnectionAccount, ConnectionDecision, DestinationRequest, NetworkError, NetworkErrorKind,
    NetworkObservationKind, RedirectChain, ScopedCredential,
};

use super::{connect, http, owner::SharedWorkerConfig};

pub(super) fn run(mut client: TcpStream, config: &SharedWorkerConfig) -> Result<(), NetworkError> {
    let timeout = Duration::from_millis(100);
    client.set_read_timeout(Some(timeout)).map_err(|_| stream_error())?;
    client.set_write_timeout(Some(timeout)).map_err(|_| stream_error())?;
    let bounds = config.plan.options().bounds();
    let account = Arc::new(Mutex::new(ConnectionAccount::new(
        bounds.connection_bytes(),
        bounds.connection_millis(),
    )));
    let mut destination = None;
    let result = execute(&mut client, config, &account, &mut destination);
    let account_value = *account.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    observe_closed(config, destination.as_ref(), &result, account_value);
    result
}

fn execute(
    client: &mut TcpStream,
    config: &SharedWorkerConfig,
    account: &Arc<Mutex<ConnectionAccount>>,
    destination: &mut Option<DestinationRequest>,
) -> Result<(), NetworkError> {
    let mut request = http::read_request(client, config.plan.options().bounds().header_bytes())?;
    if !config.token.verifies_authorization(request.routing_authorization()) {
        return Err(credential_error());
    }
    *destination = Some(request.destination.clone());
    let began = Instant::now();
    admit_request(config, &request.destination)?;
    let (mut upstream, selected) = connect::open(config, &request.destination)?;
    observe_connected(config, &request.destination, selected.address());
    if request.method == "CONNECT" {
        let response = b"HTTP/1.1 200 Connection Established\r\n\r\n";
        client.write_all(response).map_err(|_| stream_error())?;
        charge_download(account, config, response.len())?;
        return connect::tunnel(
            client.try_clone().map_err(|_| stream_error())?,
            upstream,
            account,
            config,
        );
    }
    let mut redirects = RedirectChain::new(&config.plan);
    let mut forward_body = true;
    loop {
        account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check_elapsed(elapsed(began))?;
        let credential = acquire_credential(config, &request.destination)?;
        let encoded = match &credential {
            Some((name, material)) => {
                material.expose(|bytes| request.encode(Some((name, bytes))))?
            }
            None => request.encode(None)?,
        };
        upstream.write_all(&encoded).map_err(|_| stream_error())?;
        charge_upload(account, config, encoded.len())?;
        if credential.is_some() {
            observe_credential(config, &request.destination);
        }
        if forward_body && request.content_length > 0 {
            connect::copy_exact_bounded(
                client,
                &mut upstream,
                request.content_length,
                &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
                config,
                true,
                began,
            )?;
        }
        let response =
            http::read_response(&mut upstream, config.plan.options().bounds().header_bytes())?;
        let successor = super::redirect_worker::successor(&request, &response, &mut redirects)?;
        let Some(successor) = successor else {
            client.write_all(&response.bytes).map_err(|_| stream_error())?;
            charge_download(account, config, response.bytes.len())?;
            return super::redirect_worker::copy_final_body(
                &mut upstream,
                client,
                &response,
                account,
                config,
                began,
            );
        };
        if !matches!(request.method.as_str(), "GET" | "HEAD") || request.content_length != 0 {
            return Err(redirect_error("redirect replay requires a body-free GET or HEAD request"));
        }
        super::redirect_worker::discard_body(&mut upstream, &response, account, config, began)?;
        observe_redirect(config, successor.request(), redirects.depth());
        request.follow(&successor);
        *destination = Some(request.destination.clone());
        admit_request(config, &request.destination)?;
        let (next_upstream, next_selected) = connect::open(config, &request.destination)?;
        upstream = next_upstream;
        observe_connected(config, &request.destination, next_selected.address());
        forward_body = false;
    }
}

fn admit_request(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
) -> Result<(), NetworkError> {
    let decision = config.plan.decide_request(request)?;
    observe_request(
        config,
        request,
        if decision == crate::DestinationDecision::Allowed {
            ConnectionDecision::Allowed
        } else {
            ConnectionDecision::Denied
        },
    );
    if decision == crate::DestinationDecision::Allowed {
        Ok(())
    } else {
        Err(crate::error::denied("proxy request is outside checked authority"))
    }
}

fn acquire_credential(
    config: &SharedWorkerConfig,
    destination: &DestinationRequest,
) -> Result<Option<(String, ScopedCredential)>, NetworkError> {
    let Some(credential) = &config.credential else {
        return Ok(None);
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX));
    let mut lease = credential.lease.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if lease.consume(destination, config.plan.digest(), config.plan.owner(), now).is_err() {
        return Ok(None);
    }
    let reference = lease.reference();
    let name = lease.header_name().to_owned();
    drop(lease);
    let material = credential.provider.acquire(reference)?;
    Ok(Some((name, material)))
}

fn elapsed(began: Instant) -> u64 {
    u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX)
}

const fn redirect_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Redirect,
        crate::NetworkOperation::Redirect,
        crate::RecoveryClass::Replan,
        detail,
    )
}

fn charge_upload(
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    bytes: usize,
) -> Result<(), NetworkError> {
    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
    account.lock().unwrap_or_else(std::sync::PoisonError::into_inner).charge_upload(bytes)?;
    super::owner::charge_total(
        &config.total_bytes,
        bytes,
        config.plan.options().bounds().total_bytes(),
    )
}

fn charge_download(
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    bytes: usize,
) -> Result<(), NetworkError> {
    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
    account.lock().unwrap_or_else(std::sync::PoisonError::into_inner).charge_download(bytes)?;
    super::owner::charge_total(
        &config.total_bytes,
        bytes,
        config.plan.options().bounds().total_bytes(),
    )
}

fn observe_request(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    decision: ConnectionDecision,
) {
    push(config, NetworkObservationKind::Requested, request, None, decision, 0, 0, 0);
}

fn observe_connected(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    address: std::net::IpAddr,
) {
    push(
        config,
        NetworkObservationKind::Resolved,
        request,
        Some(address),
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    );
    push(
        config,
        NetworkObservationKind::Connected,
        request,
        Some(address),
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    );
}

fn observe_credential(config: &SharedWorkerConfig, request: &DestinationRequest) {
    push(
        config,
        NetworkObservationKind::CredentialInjected,
        request,
        None,
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    );
}

fn observe_redirect(config: &SharedWorkerConfig, request: &DestinationRequest, depth: u8) {
    push(
        config,
        NetworkObservationKind::Redirected,
        request,
        None,
        ConnectionDecision::Allowed,
        depth,
        0,
        0,
    );
}

fn observe_closed(
    config: &SharedWorkerConfig,
    request: Option<&DestinationRequest>,
    result: &Result<(), NetworkError>,
    account: ConnectionAccount,
) {
    let decision = match result {
        Ok(()) => ConnectionDecision::Allowed,
        Err(error) if error.kind() == NetworkErrorKind::Denied => ConnectionDecision::Denied,
        Err(error) if error.kind() == NetworkErrorKind::Limit => ConnectionDecision::Limited,
        Err(_) if config.cancellation.is_cancelled() => ConnectionDecision::Cancelled,
        Err(_) => ConnectionDecision::Failed,
    };
    if let Some(request) = request {
        push(
            config,
            NetworkObservationKind::Closed,
            request,
            None,
            decision,
            0,
            account.uploaded(),
            account.downloaded(),
        );
    } else {
        config.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(
            NetworkObservationKind::Closed,
            None,
            None,
            None,
            None,
            decision,
            0,
            0,
            0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    config: &SharedWorkerConfig,
    kind: NetworkObservationKind,
    request: &DestinationRequest,
    address: Option<std::net::IpAddr>,
    decision: ConnectionDecision,
    depth: u8,
    uploaded: u64,
    downloaded: u64,
) {
    config.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(
        kind,
        request_name(request),
        address,
        Some(request.port()),
        Some(request.transport()),
        decision,
        depth,
        uploaded,
        downloaded,
    );
}

fn request_name(request: &DestinationRequest) -> Option<DnsName> {
    match request.host() {
        NetworkHost::Dns(name) => Some(name.clone()),
        NetworkHost::Ip(_) => None,
    }
}

const fn credential_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Credential,
        crate::NetworkOperation::Credential,
        crate::RecoveryClass::CorrectRequest,
        "proxy routing token is missing or mismatched",
    )
}

const fn stream_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Io,
        crate::NetworkOperation::Relay,
        crate::RecoveryClass::CancelAndJoin,
        "proxy stream configuration or write failed",
    )
}
