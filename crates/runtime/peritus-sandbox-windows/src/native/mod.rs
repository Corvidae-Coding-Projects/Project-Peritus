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

use crate::{HelperManifest, JobPlan, NetworkIsolation, TokenProfile, WindowsError};

pub(crate) struct Activation {
    token: token::RestrictedToken,
    job: job::OwnedJob,
    app_container: Option<token::AppContainerSid>,
    terminal: handle::TerminalAttachment,
    secrets: secret::StagedSecrets,
}

pub(crate) fn activate(
    manifest: &HelperManifest,
    inherited_job: std::fs::File,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<Activation, WindowsError> {
    handle::TerminalAttachment::validate(manifest.terminal())?;
    verify_helper_identity(manifest, should_continue)?;
    handle::verify_protected_handles(manifest)?;
    verify_network(manifest)?;
    let token = token::RestrictedToken::create(manifest.token())?;
    let app_container = match manifest.token() {
        TokenProfile::RestrictedLowIntegrity { .. } => None,
        TokenProfile::AppContainer(profile) => Some(token::AppContainerSid::derive(profile)?),
    };
    let job = job::OwnedJob::adopt(inherited_job, manifest.job())?;
    let terminal = handle::TerminalAttachment::create(manifest.terminal())?;
    let secrets = secret::stage(manifest)?;
    Ok(Activation { token, job, app_container, terminal, secrets })
}

pub(crate) fn prepare_containment_job(
    plan: JobPlan,
    object_name: &str,
) -> Result<peritus_process::NativeProtectedHandle, WindowsError> {
    let job = job::OwnedJob::create_bound(plan, object_name)?;
    peritus_process::NativeProtectedHandle::from_file(
        peritus_process::NATIVE_WINDOWS_JOB_HANDLE_LABEL,
        job.into_file(),
    )
    .map_err(|source| {
        WindowsError::new(
            crate::WindowsErrorKind::Handle,
            crate::WindowsOperation::Prepare,
            crate::WindowsRecovery::CancelAndReap,
            "retained Job Object handle cannot be staged for helper inheritance",
        )
        .with_source(crate::error::process_source(&source))
    })
}

pub(crate) fn execute(
    manifest: &HelperManifest,
    activation: &mut Activation,
) -> Result<i32, WindowsError> {
    launch::launch_and_wait(manifest, activation)
}

pub(crate) fn execute_with_channels(
    manifest: &HelperManifest,
    activation: &mut Activation,
    channels: &mut peritus_process::NativeWindowsHelperAttachment,
) -> Result<i32, WindowsError> {
    launch::launch_and_wait_with_channels(manifest, activation, channels)
}

fn verify_helper_identity(
    manifest: &HelperManifest,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), WindowsError> {
    let executable = std::env::current_exe().map_err(|_| {
        crate::error::io(crate::WindowsOperation::Activate, "helper path cannot be inspected")
    })?;
    let image = match crate::probe::inspect_helper_image(&executable, should_continue) {
        Ok(image) if image.bytes() != 0 => image,
        Ok(_) | Err(crate::probe::HelperImageFailure::Unavailable) => {
            return Err(crate::error::io(
                crate::WindowsOperation::Activate,
                "helper image cannot be streamed as one finite regular file",
            ));
        }
        Err(crate::probe::HelperImageFailure::Changed) => {
            return Err(crate::error::mismatch(
                crate::WindowsErrorKind::PreparationMismatch,
                "running helper image length changed during verification",
            ));
        }
        Err(crate::probe::HelperImageFailure::Cancelled) => {
            return Err(WindowsError::new(
                crate::WindowsErrorKind::HelperProtocol,
                crate::WindowsOperation::Activate,
                crate::WindowsRecovery::CancelAndReap,
                "running helper verification was cancelled by its retained owner",
            ));
        }
    };
    if image.digest() != manifest.helper_digest() {
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
