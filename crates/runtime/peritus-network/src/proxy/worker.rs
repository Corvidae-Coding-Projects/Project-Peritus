//! One authenticated, plan-bound proxy connection.

use std::{
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

const CANCELLATION_POLL: Duration = Duration::from_millis(250);

pub(super) fn run(mut client: TcpStream, config: &SharedWorkerConfig) -> Result<(), NetworkError> {
    client.set_read_timeout(Some(CANCELLATION_POLL)).map_err(|_| stream_error())?;
    client.set_write_timeout(Some(CANCELLATION_POLL)).map_err(|_| stream_error())?;
    let bounds = config.plan.options().bounds();
    let account = Arc::new(Mutex::new(ConnectionAccount::new(
        bounds.connection_bytes(),
        bounds.connection_millis(),
    )));
    let mut destination = None;
    let result = execute(&mut client, config, &account, &mut destination);
    let account_value = *account.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let observation = observe_closed(config, destination.as_ref(), &result, account_value);
    match (result, observation) {
        (Err(error), Err(_)) if error.is_storage() => Err(error),
        (_, Err(observation_error)) => Err(observation_error),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn execute(
    client: &mut TcpStream,
    config: &SharedWorkerConfig,
    account: &Arc<Mutex<ConnectionAccount>>,
    destination: &mut Option<DestinationRequest>,
) -> Result<(), NetworkError> {
    let bounds = config.plan.options().bounds();
    let began = Instant::now();
    let mut request = http::read_request(
        client,
        bounds.header_bytes(),
        &config.cancellation,
        bounds.connection_millis(),
        began,
    )?;
    if !request.verifies_routing(&config.token)? {
        return Err(credential_error());
    }
    *destination = Some(request.destination.clone());
    admit_request(config, &request.destination)?;
    let (mut upstream, selected) = connect::open(config, &request.destination, began)?;
    observe_connected(config, &request.destination, selected.address())?;
    if request.is_connect() {
        let response = b"HTTP/1.1 200 Connection Established\r\n\r\n";
        charge_download(
            account,
            config,
            u64::try_from(response.len()).map_err(|_| accounting_error())?,
        )?;
        connect::write_all_owned(client, response, config, began)?;
        return connect::tunnel(
            client.try_clone().map_err(|_| stream_error())?,
            upstream,
            account,
            config,
            began,
        );
    }
    let mut redirects = RedirectChain::new(&config.plan);
    let mut forward_body = true;
    loop {
        account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check_elapsed(elapsed(began)?)?;
        let credential = acquire_credential(config, &request.destination)?;
        match &credential {
            Some((name, material)) => {
                material.expose(|bytes| {
                    write_request(
                        &request,
                        &mut upstream,
                        Some((name, bytes)),
                        account,
                        config,
                        began,
                    )
                })?
            }
            None => write_request(&request, &mut upstream, None, account, config, began)?,
        }
        if credential.is_some() {
            observe_credential(config, &request.destination)?;
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
        let response = http::read_response(
            &mut upstream,
            bounds.header_bytes(),
            &config.cancellation,
            bounds.connection_millis(),
            began,
        )?;
        let successor = super::redirect_worker::successor(&request, &response, &mut redirects)?;
        let Some(successor) = successor else {
            charge_download(account, config, response.encoded_len())?;
            response.write_to(|bytes| connect::write_all_owned(client, bytes, config, began))?;
            return super::redirect_worker::copy_final_body(
                &mut upstream,
                client,
                &response,
                account,
                config,
                began,
            );
        };
        if !request.supports_redirect_replay() || request.content_length != 0 {
            return Err(redirect_error("redirect replay requires a body-free GET or HEAD request"));
        }
        super::redirect_worker::discard_body(&mut upstream, &response, account, config, began)?;
        observe_redirect(config, successor.request(), redirects.depth())?;
        request.follow(&successor, &response)?;
        *destination = Some(request.destination.clone());
        admit_request(config, &request.destination)?;
        let (next_upstream, next_selected) =
            connect::open(config, &request.destination, began)?;
        upstream = next_upstream;
        observe_connected(config, &request.destination, next_selected.address())?;
        forward_body = false;
    }
}

fn write_request(
    request: &http::RequestHead,
    upstream: &mut TcpStream,
    credential: Option<(&str, &[u8])>,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    began: Instant,
) -> Result<(), NetworkError> {
    let length = request.encoded_len(credential)?;
    charge_upload(account, config, length)?;
    let written = request.write_to(credential, |bytes| {
        connect::write_all_owned(upstream, bytes, config, began)
    })?;
    if written != length {
        return Err(accounting_error());
    }
    Ok(())
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
    )?;
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
        .map_err(|_| credential_clock_error())?;
    let now = u64::try_from(now.as_millis()).map_err(|_| credential_clock_error())?;
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

fn elapsed(began: Instant) -> Result<u64, NetworkError> {
    u64::try_from(began.elapsed().as_millis()).map_err(|_| accounting_error())
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
    bytes: u64,
) -> Result<(), NetworkError> {
    super::owner::charge(
        &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        &config.total_bytes,
        bytes,
        config.plan.options().bounds().total_bytes(),
        true,
    )
}

fn charge_download(
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    bytes: u64,
) -> Result<(), NetworkError> {
    super::owner::charge(
        &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        &config.total_bytes,
        bytes,
        config.plan.options().bounds().total_bytes(),
        false,
    )
}

fn observe_request(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    decision: ConnectionDecision,
) -> Result<(), NetworkError> {
    push(config, NetworkObservationKind::Requested, request, None, decision, 0, 0, 0)
}

fn observe_connected(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    address: std::net::IpAddr,
) -> Result<(), NetworkError> {
    push(
        config,
        NetworkObservationKind::Resolved,
        request,
        Some(address),
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    )?;
    push(
        config,
        NetworkObservationKind::Connected,
        request,
        Some(address),
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    )
}

fn observe_credential(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
) -> Result<(), NetworkError> {
    push(
        config,
        NetworkObservationKind::CredentialInjected,
        request,
        None,
        ConnectionDecision::Allowed,
        0,
        0,
        0,
    )
}

fn observe_redirect(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    depth: u8,
) -> Result<(), NetworkError> {
    push(
        config,
        NetworkObservationKind::Redirected,
        request,
        None,
        ConnectionDecision::Allowed,
        depth,
        0,
        0,
    )
}

fn observe_closed(
    config: &SharedWorkerConfig,
    request: Option<&DestinationRequest>,
    result: &Result<(), NetworkError>,
    account: ConnectionAccount,
) -> Result<(), NetworkError> {
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
        )
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
        )
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
) -> Result<(), NetworkError> {
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
    )
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

const fn accounting_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        crate::NetworkOperation::Relay,
        crate::RecoveryClass::CancelAndJoin,
        "managed proxy accounting is not representable",
    )
}

const fn credential_clock_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Credential,
        crate::NetworkOperation::Credential,
        crate::RecoveryClass::ReacquireCredential,
        "credential clock observation is not representable",
    )
}
