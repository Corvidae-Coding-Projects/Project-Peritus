//! Windows helper process entrypoint kept outside the binary composition root.

use std::process::ExitCode;

use crate::ReservedHelperExit;

/// Runs the direct-child Windows helper protocol and maps its stable process exit.
#[must_use]
pub fn helper_main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(category) => ExitCode::from(u8::try_from(category.code()).unwrap_or(120)),
    }
}

#[cfg(not(target_os = "windows"))]
const fn run() -> Result<(), ReservedHelperExit> {
    Err(ReservedHelperExit::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn run() -> Result<(), ReservedHelperExit> {
    use std::io::{self, Write};

    let mut helper_channels = peritus_process::NativeWindowsHelperAttachment::from_environment()
        .map_err(|_| ReservedHelperExit::ProtectedHandle)?;
    let mut output = io::stdout().lock();
    output
        .write_all(peritus_process::native_ready_record().as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    let manifest = read_manifest_while_owned(&helper_channels)?;
    let mut activation = {
        let inherited_job = helper_channels
            .take_containment_job()
            .ok_or(ReservedHelperExit::JobOrResource)?;
        let mut owner_connected = || helper_channels.owner_connected();
        crate::runner::activate_manifest_while(
            &manifest,
            inherited_job,
            &mut owner_connected,
        )
            .map_err(|error| classify_activation_error(&error))?
    };
    let activation_record =
        peritus_process::native_activation_record(manifest.digest(), manifest.preparation_digest());
    output
        .write_all(activation_record.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| ReservedHelperExit::Protocol)?;
    drop(output);
    let secret_files = activation
        .secret_file_identities()
        .map_err(|_| ReservedHelperExit::Secret)?;
    helper_channels
        .signal_secret_files(activation_record.into_bytes(), &secret_files)
        .and_then(|()| helper_channels.await_secret_file_adoption())
        .map_err(|_| ReservedHelperExit::Secret)?;
    let completion = crate::runner::execute_manifest_with_channels(
        &manifest,
        &mut activation,
        &mut helper_channels,
    )
    .map_err(|_| ReservedHelperExit::TargetCreate)?;
    let record = peritus_process::native_windows_completion_record(
        manifest.digest(),
        manifest.preparation_digest(),
    );
    helper_channels
        .signal_completion(record.into_bytes(), completion)
        .map_err(|_| ReservedHelperExit::TargetCreate)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn read_manifest_while_owned(
    helper_channels: &peritus_process::NativeWindowsHelperAttachment,
) -> Result<crate::HelperManifest, ReservedHelperExit> {
    use std::{sync::mpsc::RecvTimeoutError, time::Duration};

    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::Builder::new()
        .name("peritus-helper-manifest".to_owned())
        .spawn(move || {
            let result = crate::HelperManifest::read_framed(std::io::stdin().lock());
            let _ = sender.send(result);
        })
        .map_err(|_| ReservedHelperExit::Protocol)?;
    loop {
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(result) => {
                reader.join().map_err(|_| ReservedHelperExit::Protocol)?;
                return result.map_err(|_| ReservedHelperExit::Protocol);
            }
            Err(RecvTimeoutError::Timeout) if helper_channels.owner_connected() => {}
            Err(RecvTimeoutError::Timeout) => return Err(ReservedHelperExit::ProtectedHandle),
            Err(RecvTimeoutError::Disconnected) => return Err(ReservedHelperExit::Protocol),
        }
    }
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
