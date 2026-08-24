//! Exact managed-proxy handle validation and target environment projection.

use std::{ffi::OsStr, net::SocketAddr, process::Command};

use zeroize::{Zeroize, Zeroizing};

use crate::{HelperManifest, LinuxError};

use super::{close_consumed_descriptor, helper_error, read_protected_payload};

pub struct PreparedProxy {
    endpoint: SocketAddr,
    token: Zeroizing<Vec<u8>>,
}

impl PreparedProxy {
    pub fn configure(mut self, command: &mut Command) {
        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut token = Zeroizing::new(String::with_capacity(self.token.len() * 2));
        for byte in self.token.iter().copied() {
            token.push(char::from(HEX[usize::from(byte >> 4)]));
            token.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        let url = Zeroizing::new(format!("http://peritus:{}@{}", token.as_str(), self.endpoint));
        command.env("HTTP_PROXY", OsStr::new(url.as_str()));
        command.env("HTTPS_PROXY", OsStr::new(url.as_str()));
        command.env("http_proxy", OsStr::new(url.as_str()));
        command.env("https_proxy", OsStr::new(url.as_str()));
        self.token.zeroize();
    }
}

pub fn validate_handles(manifest: &HelperManifest) -> Result<(), LinuxError> {
    let handles = manifest.inherited_handles();
    let status_count =
        handles.iter().filter(|handle| handle.label() == crate::EXEC_STATUS_LABEL).count();
    if status_count > 1 {
        return Err(helper_error("helper execution-status handle is duplicated"));
    }
    let proxy_handles = handles
        .iter()
        .filter(|handle| handle.label() != crate::EXEC_STATUS_LABEL)
        .collect::<Vec<_>>();
    match manifest.network() {
        crate::NetworkIsolation::DenyAll if proxy_handles.is_empty() => Ok(()),
        crate::NetworkIsolation::ManagedProxy
            if proxy_handles.len() == 2
                && proxy_handles
                    .iter()
                    .any(|handle| handle.label() == crate::network::PROXY_LISTENER_LABEL)
                && proxy_handles
                    .iter()
                    .any(|handle| handle.label() == crate::network::PROXY_TOKEN_LABEL) =>
        {
            Ok(())
        }
        _ => Err(helper_error("managed proxy mode and exact protected handles differ")),
    }
}

pub fn prepare(manifest: &HelperManifest) -> Result<Option<PreparedProxy>, LinuxError> {
    if manifest.network() == crate::NetworkIsolation::DenyAll {
        return Ok(None);
    }
    let channel = manifest
        .inherited_handles()
        .iter()
        .find(|handle| handle.label() == crate::network::PROXY_LISTENER_LABEL)
        .ok_or_else(|| helper_error("managed proxy listener channel is absent"))?;
    let token = manifest
        .inherited_handles()
        .iter()
        .find(|handle| handle.label() == crate::network::PROXY_TOKEN_LABEL)
        .ok_or_else(|| helper_error("managed proxy routing token is absent"))?;
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| helper_error("namespace-local managed proxy listener bind failed"))?;
    let endpoint = peritus_network::send_inherited_listener(channel.descriptor(), &listener)
        .map_err(|_| helper_error("namespace-local proxy listener transfer failed"))?
        .socket_addr();
    close_consumed_descriptor(channel.descriptor())?;
    let token_bytes = read_protected_payload(token.descriptor(), 32)?;
    close_consumed_descriptor(token.descriptor())?;
    Ok(Some(PreparedProxy { endpoint, token: Zeroizing::new(token_bytes) }))
}
