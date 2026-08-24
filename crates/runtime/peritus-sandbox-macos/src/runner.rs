//! Reserved helper exits and target handoff.

use crate::{HelperManifest, MacosError, MacosErrorKind, MacosOperation, RecoveryAction};

#[cfg(any(target_os = "macos", test))]
mod materialized;

#[cfg(target_os = "macos")]
use peritus_process::NativePtyAttachment;

#[cfg(target_os = "macos")]
const SECRET_HANDLES_ENV: &str = "PERITUS_NATIVE_SECRET_HANDLES_V1";

/// Stable fallback process exits paired with authenticated pre-exec failure status.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReservedHelperExit {
    /// Ready/manifest framing or checksum failure.
    Protocol = 120,
    /// Seatbelt profile activation failed.
    SandboxDenied = 121,
    /// A required resource control could not be installed.
    ResourceControl = 122,
    /// The literal target could not be executed.
    TargetExec = 123,
    /// A required inherited proxy or secret channel was absent.
    ProtectedChannel = 124,
    /// The current platform cannot execute this helper.
    UnsupportedPlatform = 125,
}

impl ReservedHelperExit {
    /// Returns the numeric helper exit value.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Protocol => 120,
            Self::SandboxDenied => 121,
            Self::ResourceControl => 122,
            Self::TargetExec => 123,
            Self::ProtectedChannel => 124,
            Self::UnsupportedPlatform => 125,
        }
    }

    /// Decodes only a pre-activation helper fallback status, never a target termination.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            120 => Some(Self::Protocol),
            121 => Some(Self::SandboxDenied),
            122 => Some(Self::ResourceControl),
            123 => Some(Self::TargetExec),
            124 => Some(Self::ProtectedChannel),
            125 => Some(Self::UnsupportedPlatform),
            _ => None,
        }
    }
}

/// Literal target command with protected environment/file/handle delivery already staged.
#[cfg(target_os = "macos")]
pub struct PreparedTargetCommand {
    command: std::process::Command,
    pty_required: bool,
    _materialized_secret_files: materialized::MaterializedSecretFiles,
}

/// Runs a decoded helper manifest and replaces the helper with the literal target.
///
/// # Errors
/// Returns only before target replacement when a native control cannot be installed.
#[cfg(target_os = "macos")]
pub fn execute_manifest(manifest: &HelperManifest) -> Result<(), MacosError> {
    let target = prepare_target_command(manifest)?;
    execute_prepared_target(target, None)
}

/// Replaces the helper with the literal target and an optional C2-owned PTY attachment.
///
/// # Errors
/// Returns only before target replacement when terminal mapping or native configuration fails.
#[cfg(target_os = "macos")]
pub fn execute_manifest_with_pty(
    manifest: &HelperManifest,
    attachment: Option<NativePtyAttachment>,
) -> Result<(), MacosError> {
    let target = prepare_target_command(manifest)?;
    execute_prepared_target(target, attachment)
}

/// Reads protected payloads and stages their exact target destinations before Seatbelt activation.
///
/// # Errors
/// Returns a typed fail-closed error for a missing/truncated handle or unsafe destination.
#[cfg(target_os = "macos")]
pub fn prepare_target_command(
    manifest: &HelperManifest,
) -> Result<PreparedTargetCommand, MacosError> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _, process::Command};
    use zeroize::Zeroize as _;

    native::verify_protected_channels(manifest)?;
    let mut command = Command::new(manifest.target_executable());
    let mut materialized_secret_files = materialized::MaterializedSecretFiles::new();
    command.args(manifest.target_arguments()).current_dir(manifest.working_directory()).env_clear();
    for entry in manifest.environment() {
        command.env(entry.name(), entry.value());
    }
    if let Some(proxy) = manifest.proxy_descriptor() {
        let mut token =
            native::read_protected_payload(proxy.route().routing_handle(), proxy.payload_len())?;
        if token.len() != 32 {
            token.zeroize();
            return Err(protected_delivery_error(
                "managed proxy routing token has an invalid length",
            ));
        }
        let mut token_hex = hex_bytes(&token);
        token.zeroize();
        let mut proxy_url = format!("http://peritus:{token_hex}@{}", proxy.route().endpoint());
        token_hex.zeroize();
        command.env("HTTP_PROXY", &proxy_url).env("HTTPS_PROXY", &proxy_url);
        proxy_url.zeroize();
    }
    let mut brokered = Vec::new();
    for secret in manifest.secrets() {
        match secret.destination() {
            crate::SecretHandleDestination::Environment(name) => {
                let mut payload =
                    native::read_protected_payload(secret.descriptor(), secret.payload_len())?;
                if payload.contains(&0) {
                    payload.zeroize();
                    return Err(protected_delivery_error(
                        "secret environment payload contains NUL",
                    ));
                }
                command.env(name.as_str(), OsString::from_vec(payload.clone()));
                payload.zeroize();
            }
            crate::SecretHandleDestination::File(path) => {
                let mut payload =
                    native::read_protected_payload(secret.descriptor(), secret.payload_len())?;
                let result = native::materialize_secret_file(path.as_str(), &payload);
                payload.zeroize();
                result?;
                materialized_secret_files.record(path.as_str());
            }
            crate::SecretHandleDestination::Brokered(label) => {
                brokered.push(format!(
                    "{}:{}",
                    hex_bytes(label.as_str().as_bytes()),
                    secret.descriptor()
                ));
            }
        }
    }
    if !brokered.is_empty() {
        command.env(SECRET_HANDLES_ENV, brokered.join(","));
    }
    Ok(PreparedTargetCommand {
        command,
        pty_required: matches!(manifest.terminal(), crate::TerminalMapping::Pty { .. }),
        _materialized_secret_files: materialized_secret_files,
    })
}

/// Replaces the helper with one already-staged literal target command.
///
/// # Errors
/// Returns only when PTY configuration or literal target exec fails.
#[cfg(target_os = "macos")]
pub fn execute_prepared_target(
    mut target: PreparedTargetCommand,
    attachment: Option<NativePtyAttachment>,
) -> Result<(), MacosError> {
    use std::os::unix::process::CommandExt as _;

    if target.pty_required != attachment.is_some() {
        return Err(protected_delivery_error(
            "C2-owned PTY attachment differs from prepared target mapping",
        ));
    }
    if let Some(attachment) = attachment {
        attachment
            .configure(&mut target.command)
            .map_err(|source| crate::error::io_error(MacosOperation::Activate, &source))?;
    }
    let error = target.command.exec();
    Err(crate::error::io_error(MacosOperation::Activate, &error))
}

/// Verifies protected channels and installs every native control before activation is announced.
///
/// # Errors
/// Returns a typed fail-closed error if a channel, rlimit, or Seatbelt activation fails.
#[cfg(target_os = "macos")]
pub fn activate_manifest(manifest: &HelperManifest) -> Result<(), MacosError> {
    activate_manifest_with_pty(manifest, None)
}

/// Installs native controls while retaining an optional exact C2-owned PTY slave handle.
///
/// # Errors
/// Returns a typed fail-closed error for a terminal mismatch, channel, rlimit, or Seatbelt failure.
#[cfg(target_os = "macos")]
pub fn activate_manifest_with_pty(
    manifest: &HelperManifest,
    attachment: Option<&NativePtyAttachment>,
) -> Result<(), MacosError> {
    use std::os::fd::AsRawFd;

    validate_terminal_attachment(manifest, attachment.is_some())?;
    let retained_pty = attachment
        .map(|attachment| u32::try_from(attachment.as_raw_fd()))
        .transpose()
        .map_err(|_| {
            MacosError::new(
                MacosErrorKind::HelperFailure,
                MacosOperation::Activate,
                RecoveryAction::CancelAndReap,
                "C2-owned PTY attachment has an invalid descriptor",
            )
        })?;
    native::verify_protected_channels(manifest)?;
    native::close_unrelated_descriptors(manifest, retained_pty)?;
    native::mark_exec_status_close_on_exec(manifest.exec_status_descriptor())?;
    native::install_resource_controls(manifest.resources())?;
    native::install_seatbelt(manifest.profile())
}

#[cfg(target_os = "macos")]
fn validate_terminal_attachment(
    manifest: &HelperManifest,
    attached: bool,
) -> Result<(), MacosError> {
    let matches = match manifest.terminal() {
        crate::TerminalMapping::Pipes { .. } => !attached,
        crate::TerminalMapping::Pty { .. } => attached,
    };
    if matches {
        Ok(())
    } else {
        Err(MacosError::new(
            MacosErrorKind::HelperFailure,
            MacosOperation::Activate,
            RecoveryAction::CancelAndReap,
            "C2-owned PTY attachment differs from the checked terminal mapping",
        ))
    }
}

#[cfg(target_os = "macos")]
fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

#[cfg(target_os = "macos")]
fn protected_delivery_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Activate,
        RecoveryAction::CancelAndReap,
        detail,
    )
}

/// Returns strict unsupported behavior on non-macOS hosts.
///
/// # Errors
/// Always returns `UnsupportedHost` outside macOS.
#[cfg(not(target_os = "macos"))]
pub fn execute_manifest(_manifest: &HelperManifest) -> Result<(), MacosError> {
    Err(MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "macOS helper cannot execute on this platform",
    ))
}

/// Returns strict unsupported activation outside macOS.
///
/// # Errors
/// Always returns `UnsupportedHost` outside macOS.
#[cfg(not(target_os = "macos"))]
pub fn activate_manifest(_manifest: &HelperManifest) -> Result<(), MacosError> {
    Err(MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "macOS controls cannot be activated on this platform",
    ))
}

#[cfg(target_os = "macos")]
mod native;
