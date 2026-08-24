//! Per-request redirect successor selection and bounded response draining.

use std::{
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::{ConnectionAccount, NetworkError, RedirectChain, RedirectTarget};

use super::{connect, http, owner::SharedWorkerConfig};

pub(super) fn successor(
    request: &http::RequestHead,
    response: &http::ResponseHead,
    chain: &mut RedirectChain<'_>,
) -> Result<Option<RedirectTarget>, NetworkError> {
    if !(300..400).contains(&response.status) {
        return Ok(None);
    }
    let Some(location) = response.location.as_deref() else {
        return Ok(None);
    };
    let target = if location.starts_with('/') {
        RedirectTarget::relative(request.destination.clone(), location)?
    } else {
        RedirectTarget::parse(location)?
    };
    chain.follow(target).map(Some)
}

pub(super) fn copy_final_body(
    upstream: &mut TcpStream,
    client: &mut TcpStream,
    response: &http::ResponseHead,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    began: Instant,
) -> Result<(), NetworkError> {
    match response.content_length {
        Some(bytes) => connect::copy_exact_bounded(
            upstream,
            client,
            bytes,
            &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            config,
            false,
            began,
        ),
        None => connect::copy_to_eof_bounded(
            upstream,
            client,
            &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            config,
            false,
            began,
        ),
    }
}

pub(super) fn discard_body(
    upstream: &mut TcpStream,
    response: &http::ResponseHead,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    began: Instant,
) -> Result<(), NetworkError> {
    let mut sink = std::io::sink();
    match response.content_length {
        Some(bytes) => connect::copy_exact_bounded(
            upstream,
            &mut sink,
            bytes,
            &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            config,
            false,
            began,
        ),
        None => connect::copy_to_eof_bounded(
            upstream,
            &mut sink,
            &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            config,
            false,
            began,
        ),
    }
}
