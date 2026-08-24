//! Authorized managed-proxy owner and protected-handle preparation.

use crate::{
    InheritedHandle, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery,
    network::ManagedProxyOwner,
};
use peritus_process::NativeProtectedHandle;

type PreparedProxy = (Option<ManagedProxyOwner>, Vec<NativeProtectedHandle>, Vec<InheritedHandle>);

#[cfg(unix)]
pub fn prepare(
    sandbox: &peritus_sandbox::CheckedSandboxPlan,
    preparation: &mut Option<peritus_network::ManagedProxyPreparation>,
) -> Result<PreparedProxy, LinuxError> {
    if sandbox.requirements().network().is_empty() {
        return Ok((None, Vec::new(), Vec::new()));
    }
    let preparation = preparation.take().ok_or_else(|| {
        LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Prepare,
            LinuxRecovery::ConfigureHost,
            "egress requires an inert managed-proxy preparation",
        )
    })?;
    let mut proxy = preparation.prepare_inherited_listener(sandbox).map_err(|_| {
        LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Prepare,
            LinuxRecovery::CancelAndReap,
            "managed netns proxy owner preparation failed",
        )
    })?;
    let listener = proxy.take_listener_channel().map_err(|_| {
        LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Prepare,
            LinuxRecovery::CancelAndReap,
            "managed netns proxy listener channel transfer failed",
        )
    })?;
    let listener_handle = NativeProtectedHandle::from_file(crate::PROXY_LISTENER_LABEL, listener)
        .map_err(|_| proxy_error("proxy listener handle creation failed"))?;
    let token_handle = proxy
        .routing_token()
        .expose_bytes(|bytes| {
            NativeProtectedHandle::from_bytes(crate::PROXY_TOKEN_LABEL, bytes.to_vec())
        })
        .map_err(|_| proxy_error("proxy token handle creation failed"))?;
    let inherited = crate::canonical_handles(vec![
        InheritedHandle::new(listener_handle.raw_handle(), listener_handle.label().to_owned())?,
        InheritedHandle::new(token_handle.raw_handle(), token_handle.label().to_owned())?,
    ])?;
    Ok((Some(proxy), vec![listener_handle, token_handle], inherited))
}

#[cfg(not(unix))]
pub fn prepare(
    sandbox: &peritus_sandbox::CheckedSandboxPlan,
    _preparation: &mut Option<peritus_network::ManagedProxyPreparation>,
) -> Result<PreparedProxy, LinuxError> {
    if sandbox.requirements().network().is_empty() {
        Ok((None, Vec::new(), Vec::new()))
    } else {
        Err(LinuxError::new(
            LinuxErrorKind::UnsupportedHost,
            LinuxOperation::Prepare,
            LinuxRecovery::ConfigureHost,
            "Linux inherited-listener proxy preparation is unavailable on this build target",
        ))
    }
}

#[cfg(unix)]
fn proxy_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Network,
        LinuxOperation::Prepare,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}
