//! Direct-child helper protocol orchestration.

use std::process::ExitCode;

use crate::ReservedHelperExit;
#[cfg(target_os = "macos")]
use crate::{
    HelperManifest, MacosErrorKind, activate_manifest_with_pty, execute_prepared_target,
    prepare_target_command,
};
#[cfg(target_os = "macos")]
use peritus_process::{NativePtyAttachment, native_activation_record, native_ready_record};
#[cfg(target_os = "macos")]
use std::{
    fs::File,
    io::{self, Write},
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
    // The duplicate is opened before readiness and consumes only the bounded manifest frame.
    let input = File::open("/dev/fd/0").map_err(|_| ReservedHelperExit::Protocol)?;
    let mut output = io::stdout().lock();
    output
        .write_all(native_ready_record().as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;

    let manifest = HelperManifest::read_framed(input).map_err(|_| ReservedHelperExit::Protocol)?;
    let target = prepare_target_command(&manifest).map_err(classify_activation_error)?;
    activate_manifest_with_pty(&manifest, pty.as_ref()).map_err(classify_activation_error)?;

    let activation = native_activation_record(manifest.digest(), manifest.preparation_digest());
    output
        .write_all(activation.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    drop(output);
    if execute_prepared_target(target, pty).is_err() {
        crate::exec_status::report_helper_failure(
            manifest.exec_status_descriptor(),
            manifest.digest(),
            manifest.preparation_digest(),
        )
        .map_err(|_| ReservedHelperExit::ProtectedChannel)?;
        return Err(ReservedHelperExit::TargetExec);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn classify_activation_error(error: crate::MacosError) -> ReservedHelperExit {
    classify_activation_kind(error.kind())
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
