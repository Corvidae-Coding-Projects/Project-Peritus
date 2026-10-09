//! Inventoried Windows FFI boundary for native probe, activation, and target creation.

#![allow(
    unsafe_code,
    reason = "Windows token, Job Object, attribute-list, and process APIs require audited FFI"
)]

mod handle;
mod job;
mod launch;
pub(crate) mod path;
pub(crate) mod probe;
mod secret;
mod token;
pub(crate) mod wfp;

pub(crate) use token::derive_profile;

use crate::{HelperManifest, NetworkIsolation, TokenProfile, WindowsError};

pub(crate) struct Activation {
    token: token::RestrictedToken,
    job: job::OwnedJob,
    app_container: Option<token::AppContainerSid>,
    terminal: handle::TerminalAttachment,
    secrets: secret::StagedSecrets,
}

pub(crate) fn activate(manifest: &HelperManifest) -> Result<Activation, WindowsError> {
    verify_helper_identity(manifest)?;
    handle::verify_protected_handles(manifest)?;
    verify_network(manifest)?;
    let token = token::RestrictedToken::create(manifest.token())?;
    let app_container = match manifest.token() {
        TokenProfile::RestrictedLowIntegrity { .. } => None,
        TokenProfile::AppContainer(profile) => Some(token::AppContainerSid::derive(profile)?),
    };
    let job = job::OwnedJob::create(manifest.job())?;
    let terminal = handle::TerminalAttachment::create(manifest.terminal())?;
    let secrets = secret::stage(manifest)?;
    Ok(Activation { token, job, app_container, terminal, secrets })
}

pub(crate) fn execute(
    manifest: &HelperManifest,
    activation: &Activation,
) -> Result<i32, WindowsError> {
    launch::launch_and_wait(manifest, activation)
}

pub(crate) fn execute_with_channels(
    manifest: &HelperManifest,
    activation: &Activation,
    channels: &mut peritus_process::NativeWindowsHelperAttachment,
) -> Result<i32, WindowsError> {
    launch::launch_and_wait_with_channels(manifest, activation, channels)
}

fn verify_helper_identity(manifest: &HelperManifest) -> Result<(), WindowsError> {
    let executable = std::env::current_exe().map_err(|_| {
        crate::error::io(crate::WindowsOperation::Activate, "helper path cannot be inspected")
    })?;
    let bytes = std::fs::read(executable).map_err(|_| {
        crate::error::io(crate::WindowsOperation::Activate, "helper image cannot be read")
    })?;
    if peritus_codec::sha256(&bytes) != manifest.helper_digest() {
        return Err(crate::error::mismatch(
            crate::WindowsErrorKind::PreparationMismatch,
            "running helper image differs from the probed identity",
        ));
    }
    Ok(())
}

fn verify_network(manifest: &HelperManifest) -> Result<(), WindowsError> {
    match manifest.network() {
        NetworkIsolation::DenyAll if manifest.token().is_app_container() => Ok(()),
        NetworkIsolation::ManagedProxy(route)
            if manifest.token().is_app_container()
                && route.endpoint().ip().is_loopback()
                && route.network_plan_digest() == manifest.plan_digest() =>
        {
            Ok(())
        }
        NetworkIsolation::DenyAll => Err(WindowsError::new(
            crate::WindowsErrorKind::Network,
            crate::WindowsOperation::Activate,
            crate::WindowsRecovery::ConfigureHost,
            "deny-all networking requires AppContainer isolation",
        )),
        NetworkIsolation::ManagedProxy(_) => Err(WindowsError::new(
            crate::WindowsErrorKind::Network,
            crate::WindowsOperation::Activate,
            crate::WindowsRecovery::Reauthorize,
            "managed proxy route is not bound to AppContainer and the checked network plan",
        )),
    }
}
