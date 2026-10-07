//! Direct-child helper protocol orchestration.

use std::process::ExitCode;

use crate::ReservedHelperExit;
#[cfg(target_os = "macos")]
use crate::{
    HelperManifest, MacosErrorKind, activate_manifest_with_pty_report, execute_prepared_target,
};
#[cfg(target_os = "macos")]
use peritus_process::{NativePtyAttachment, native_activation_record, native_ready_record};
#[cfg(target_os = "macos")]
use std::{
    fs::File,
    os::fd::AsRawFd as _,
};

/// Runs the native helper protocol and returns its reserved process exit.
#[must_use]
pub fn run_helper_process() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(category) => ExitCode::from(u8::try_from(category.code()).unwrap_or(120)),
    }
}

#[cfg(not(target_os = "macos"))]
const fn run() -> Result<(), ReservedHelperExit> {
    Err(ReservedHelperExit::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn run() -> Result<(), ReservedHelperExit> {
    let pty = NativePtyAttachment::from_environment()
        .map_err(|_| ReservedHelperExit::ProtectedChannel)?;
    let owner = crate::runner::parent_process();
    let mut owner_alive = || crate::runner::parent_process_is(owner);
    // The duplicate is opened before readiness and consumes only the bounded manifest frame.
    let mut input = File::open("/dev/fd/0").map_err(|_| ReservedHelperExit::Protocol)?;
    crate::runner::write_status_while(
        1,
        native_ready_record().as_bytes(),
        &mut owner_alive,
    )
        .map_err(|_| ReservedHelperExit::Protocol)?;

    let manifest = {
        let _nonblocking = crate::runner::make_nonblocking(input.as_raw_fd())
            .map_err(|_| ReservedHelperExit::ProtectedChannel)?;
        HelperManifest::read_framed_while(&mut input, &mut owner_alive)
        .map_err(|_| ReservedHelperExit::Protocol)?
    };
    let target = crate::runner::prepare_target_command_while(&manifest, &mut owner_alive)
        .map_err(|error| classify_activation_kind(error.kind()))?;
    let target = match activate_manifest_with_pty_report(&manifest, pty.as_ref()) {
        Ok(_resources) => target,
        Err(error) => {
            let error = target.cleanup_after_failure(error);
            return Err(classify_activation_kind(error.kind()));
        }
    };

    let activation = native_activation_record(manifest.digest(), manifest.preparation_digest());
    if let Err(error) = crate::runner::write_status_while(
        1,
        activation.as_bytes(),
        &mut owner_alive,
    ) {
        let error = target.cleanup_after_failure(error);
        return Err(classify_activation_kind(error.kind()));
    }
    if execute_prepared_target(target, pty).is_err() {
        crate::exec_status::report_helper_failure_while(
            manifest.exec_status_descriptor(),
            manifest.digest(),
            manifest.preparation_digest(),
            &mut owner_alive,
        )
        .map_err(|_| ReservedHelperExit::ProtectedChannel)?;
        return Err(ReservedHelperExit::TargetExec);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
const fn classify_activation_kind(kind: MacosErrorKind) -> ReservedHelperExit {
    match kind {
        MacosErrorKind::SandboxDenied | MacosErrorKind::ProfileCompilation => {
            ReservedHelperExit::SandboxDenied
        }
        MacosErrorKind::ResourceLimit => ReservedHelperExit::ResourceControl,
        MacosErrorKind::UnsupportedHost => ReservedHelperExit::UnsupportedPlatform,
        MacosErrorKind::HelperFailure => ReservedHelperExit::ProtectedChannel,
        _ => ReservedHelperExit::Protocol,
    }
}
