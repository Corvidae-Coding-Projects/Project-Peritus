//! Reserved helper exits and target handoff.

use crate::{
    HelperManifest, MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    ResourceControlPlan,
};
use peritus_sandbox::SandboxResourceKind;

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

/// One exact rlimit installed for a selected hard-enforced resource dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeResourceCeiling {
    kind: SandboxResourceKind,
    effective: u64,
    inherited_soft: u64,
    inherited_hard: u64,
}

impl NativeResourceCeiling {
    #[cfg(target_os = "macos")]
    pub(crate) const fn new(
        kind: SandboxResourceKind,
        effective_ceiling: u64,
        inherited_soft_ceiling: u64,
        inherited_hard_ceiling: u64,
    ) -> Self {
        Self {
            kind,
            effective: effective_ceiling,
            inherited_soft: inherited_soft_ceiling,
            inherited_hard: inherited_hard_ceiling,
        }
    }

    /// Returns the selected resource dimension.
    #[must_use]
    pub const fn kind(self) -> SandboxResourceKind {
        self.kind
    }

    /// Returns the exact installed ceiling in the checked plan's unit.
    #[must_use]
    pub const fn effective_ceiling(self) -> u64 {
        self.effective
    }

    /// Returns the inherited soft ceiling observed before installation.
    #[must_use]
    pub const fn inherited_soft_ceiling(self) -> u64 {
        self.inherited_soft
    }

    /// Returns the inherited operating-system hard ceiling used during negotiation.
    #[must_use]
    pub const fn inherited_hard_ceiling(self) -> u64 {
        self.inherited_hard
    }
}

/// Exact native ceilings installed before the helper announces activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeResourceControlReport {
    controls: Vec<NativeResourceCeiling>,
}

impl NativeResourceControlReport {
    #[cfg(target_os = "macos")]
    pub(crate) fn new(controls: Vec<NativeResourceCeiling>) -> Self {
        Self { controls }
    }

    /// Returns selected hard controls in stable resource-plan order.
    #[must_use]
    pub fn controls(&self) -> &[NativeResourceCeiling] {
        &self.controls
    }
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
    materialized_secret_files: materialized::MaterializedSecretFiles,
}

#[cfg(target_os = "macos")]
impl PreparedTargetCommand {
    pub(crate) fn cleanup_after_failure(mut self, original: MacosError) -> MacosError {
        self.materialized_secret_files.cleanup().err().unwrap_or(original)
    }
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
    prepare_target_command_while(manifest, || true)
}

/// Reads and stages protected payloads while the execution owner permits continued work.
///
/// # Errors
/// Returns a typed fail-closed error for owner cancellation, a missing or truncated handle,
/// incomplete rollback, or an unsafe destination.
#[cfg(target_os = "macos")]
pub fn prepare_target_command_while(
    manifest: &HelperManifest,
    mut should_continue: impl FnMut() -> bool,
) -> Result<PreparedTargetCommand, MacosError> {
    let mut materialized_secret_files = materialized::MaterializedSecretFiles::new();
    match stage_target_command(
        manifest,
        &mut materialized_secret_files,
        &mut should_continue,
    ) {
        Ok((command, pty_required)) => Ok(PreparedTargetCommand {
            command,
            pty_required,
            materialized_secret_files,
        }),
        Err(original) => {
            Err(materialized_secret_files.cleanup().err().unwrap_or(original))
        }
    }
}

#[cfg(target_os = "macos")]
fn stage_target_command(
    manifest: &HelperManifest,
    materialized_secret_files: &mut materialized::MaterializedSecretFiles,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(std::process::Command, bool), MacosError> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _, process::Command};
    use zeroize::Zeroize as _;

    native::ensure_staging_continues(should_continue)?;
    native::verify_protected_channels(manifest)?;
    let mut command = Command::new(manifest.target_executable());
    let admitted_command = peritus_process::CommandSpec::new(
        manifest.target_executable().to_owned(),
        manifest.target_arguments().to_vec(),
    )
    .map_err(|_| protected_delivery_error("target command failed native helper validation"))?;
    let mut admitted_variables = manifest
        .environment()
        .iter()
        .map(|entry| {
            peritus_process::EnvironmentVariable::new(
                entry.name().to_owned(),
                entry.value().to_owned(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| protected_delivery_error("target environment failed native validation"))?;
    command.args(manifest.target_arguments()).current_dir(manifest.working_directory()).env_clear();
    for entry in manifest.environment() {
        command.env(entry.name(), entry.value());
    }
    if let Some(proxy) = manifest.proxy_descriptor() {
        let mut token = native::read_protected_payload_while(
            proxy.route().routing_handle(),
            proxy.payload_len(),
            should_continue,
        )?;
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
        admitted_variables.push(
            peritus_process::EnvironmentVariable::new("HTTP_PROXY", proxy_url.clone())
                .map_err(|_| protected_delivery_error("proxy environment is invalid"))?,
        );
        admitted_variables.push(
            peritus_process::EnvironmentVariable::new("HTTPS_PROXY", proxy_url.clone())
                .map_err(|_| protected_delivery_error("proxy environment is invalid"))?,
        );
        proxy_url.zeroize();
    }
    let mut brokered = Vec::new();
    for secret in manifest.secrets() {
        native::ensure_staging_continues(should_continue)?;
        match secret.destination() {
            crate::SecretHandleDestination::Environment(name) => {
                let mut payload = native::read_protected_payload_while(
                    secret.descriptor(),
                    secret.payload_len(),
                    should_continue,
                )?;
                if payload.contains(&0) {
                    payload.zeroize();
                    return Err(protected_delivery_error(
                        "secret environment payload contains NUL",
                    ));
                }
                let value = OsString::from_vec(payload.clone());
                command.env(name.as_str(), &value);
                admitted_variables.push(
                    peritus_process::EnvironmentVariable::new(name.as_str(), value)
                        .map_err(|_| protected_delivery_error("secret environment is invalid"))?,
                );
                payload.zeroize();
            }
            crate::SecretHandleDestination::File(path) => {
                native::materialize_secret_file(
                    secret.descriptor(),
                    secret.payload_len(),
                    path.as_str(),
                    materialized_secret_files,
                    should_continue,
                )?;
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
        let value = brokered.join(",");
        command.env(SECRET_HANDLES_ENV, &value);
        admitted_variables.push(
            peritus_process::EnvironmentVariable::new(SECRET_HANDLES_ENV, value)
                .map_err(|_| protected_delivery_error("secret handle environment is invalid"))?,
        );
    }
    let admitted_environment = peritus_process::EnvironmentPlan::cleared(admitted_variables)
        .map_err(|_| protected_delivery_error("target environment failed native validation"))?;
    peritus_process::validate_native_command_environment(
        &admitted_command,
        &admitted_environment,
    )
    .map_err(|_| protected_delivery_error("target exceeds current native exec capacity"))?;
    native::ensure_staging_continues(should_continue)?;
    Ok((
        command,
        matches!(manifest.terminal(), crate::TerminalMapping::Pty { .. }),
    ))
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
        let error = protected_delivery_error(
            "C2-owned PTY attachment differs from prepared target mapping",
        );
        return Err(target.cleanup_after_failure(error));
    }
    if let Some(attachment) = attachment {
        if let Err(source) = attachment.configure(&mut target.command) {
            let error = crate::error::io_error(MacosOperation::Activate, &source);
            return Err(target.cleanup_after_failure(error));
        }
    }
    let error = target.command.exec();
    let error = crate::error::io_error(MacosOperation::Activate, &error);
    Err(target.cleanup_after_failure(error))
}

/// Verifies protected channels and installs every native control before activation is announced.
///
/// # Errors
/// Returns a typed fail-closed error if a channel, rlimit, or Seatbelt activation fails.
#[cfg(target_os = "macos")]
pub fn activate_manifest(
    manifest: &HelperManifest,
) -> Result<(), MacosError> {
    activate_manifest_report(manifest).map(drop)
}

/// Installs native controls and reports their exact effective ceilings before activation is
/// announced.
///
/// # Errors
/// Returns a typed fail-closed error if a channel, rlimit, or Seatbelt activation fails.
#[cfg(target_os = "macos")]
pub fn activate_manifest_report(
    manifest: &HelperManifest,
) -> Result<NativeResourceControlReport, MacosError> {
    activate_manifest_with_pty_report(manifest, None)
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
    activate_manifest_with_pty_report(manifest, attachment).map(drop)
}

/// Installs native controls, retaining an optional PTY, and returns the exact ceiling report.
///
/// # Errors
/// Returns a typed fail-closed error for a terminal mismatch, channel, rlimit, or Seatbelt failure.
#[cfg(target_os = "macos")]
pub fn activate_manifest_with_pty_report(
    manifest: &HelperManifest,
    attachment: Option<&NativePtyAttachment>,
) -> Result<NativeResourceControlReport, MacosError> {
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
    let resources = native::install_resource_controls(manifest.resources())?;
    native::install_seatbelt(manifest.profile())?;
    Ok(resources)
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
pub fn activate_manifest(
    _manifest: &HelperManifest,
) -> Result<(), MacosError> {
    Err(MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "macOS controls cannot be activated on this platform",
    ))
}

/// Returns strict unsupported activation reporting outside macOS.
///
/// # Errors
/// Always returns `UnsupportedHost` outside macOS.
#[cfg(not(target_os = "macos"))]
pub fn activate_manifest_report(
    _manifest: &HelperManifest,
) -> Result<NativeResourceControlReport, MacosError> {
    Err(MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "macOS controls cannot be activated on this platform",
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn negotiate_resource_controls(
    controls: &ResourceControlPlan,
) -> Result<ResourceControlPlan, MacosError> {
    native::negotiate_resource_controls(controls)
}

#[cfg(not(target_os = "macos"))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "macOS preparation performs fallible inherited-hard-limit negotiation"
)]
pub(crate) fn negotiate_resource_controls(
    controls: &ResourceControlPlan,
) -> Result<ResourceControlPlan, MacosError> {
    Ok(controls.clone())
}

#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
pub(crate) use native::{make_nonblocking, parent_process, parent_process_is, write_status_while};
