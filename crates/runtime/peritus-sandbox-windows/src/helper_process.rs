//! Windows helper process entrypoint kept outside the binary composition root.

use std::process::ExitCode;

use crate::ReservedHelperExit;

/// Runs the direct-child Windows helper protocol and maps its stable process exit.
#[must_use]
pub fn helper_main() -> ExitCode {
    match run() {
        Ok(code) => u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from),
        Err(category) => ExitCode::from(u8::try_from(category.code()).unwrap_or(120)),
    }
}

#[cfg(not(target_os = "windows"))]
const fn run() -> Result<i32, ReservedHelperExit> {
    Err(ReservedHelperExit::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn run() -> Result<i32, ReservedHelperExit> {
    use std::io::{self, Write};

    let mut output = io::stdout().lock();
    output
        .write_all(peritus_process::native_ready_record().as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    let manifest = crate::HelperManifest::read_framed(io::stdin().lock())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    let mut helper_channels = peritus_process::NativeWindowsHelperAttachment::from_environment()
        .map_err(|_| ReservedHelperExit::ProtectedHandle)?;
    let activation =
        crate::activate_manifest(&manifest).map_err(|error| classify_activation_error(&error))?;
    let activation_record =
        peritus_process::native_activation_record(manifest.digest(), manifest.preparation_digest());
    output
        .write_all(activation_record.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    drop(output);
    crate::runner::execute_manifest_with_channels(&manifest, &activation, &mut helper_channels)
        .map_err(|_| ReservedHelperExit::TargetCreate)
}

#[cfg(target_os = "windows")]
const fn classify_activation_error(error: &crate::WindowsError) -> ReservedHelperExit {
    match error.kind() {
        crate::WindowsErrorKind::Token | crate::WindowsErrorKind::AppContainer => {
            ReservedHelperExit::Token
        }
        crate::WindowsErrorKind::Job | crate::WindowsErrorKind::Resource => {
            ReservedHelperExit::JobOrResource
        }
        crate::WindowsErrorKind::Handle => ReservedHelperExit::ProtectedHandle,
        crate::WindowsErrorKind::Network => ReservedHelperExit::Network,
        crate::WindowsErrorKind::Secret => ReservedHelperExit::Secret,
        crate::WindowsErrorKind::UnsupportedHost => ReservedHelperExit::UnsupportedPlatform,
        _ => ReservedHelperExit::Protocol,
    }
}
