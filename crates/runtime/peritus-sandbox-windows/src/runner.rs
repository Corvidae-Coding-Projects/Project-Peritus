//! Structured helper launch and reserved pre-target failure categories.

use peritus_process::{CommandSpec, NativeLaunchDescription, NativeProtectedHandle};

use crate::{
    HelperManifest, InheritedHandlePolicy, JobPlan, TerminalMapping, TokenProfile, WindowsError,
    WindowsErrorKind, WindowsOperation, WindowsRecovery,
};

/// Installed native controls retained until target completion.
#[cfg(target_os = "windows")]
pub struct WindowsActivation {
    inner: crate::native::Activation,
}

/// Unconstructable activation marker on unsupported hosts.
#[cfg(not(target_os = "windows"))]
pub struct WindowsActivation {
    _private: (),
}

/// Backend-local view of the reviewed direct-child launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsLaunchDescription {
    command: CommandSpec,
    helper_identity: String,
    manifest: HelperManifest,
    handles: InheritedHandlePolicy,
    token: TokenProfile,
    job: JobPlan,
    terminal: TerminalMapping,
}

impl WindowsLaunchDescription {
    /// Builds a no-shell helper launch with no model-controlled helper arguments.
    ///
    /// # Errors
    /// Rejects an invalid helper command or C2 launch description.
    pub fn new(
        helper_path: &std::path::Path,
        helper_identity: String,
        manifest: HelperManifest,
        protected_handles: Vec<NativeProtectedHandle>,
    ) -> Result<(Self, NativeLaunchDescription), WindowsError> {
        let helper_text =
            helper_path.to_str().ok_or_else(|| helper_error("helper path is not UTF-8"))?;
        let command = CommandSpec::new(helper_text, std::iter::empty::<String>())
            .map_err(|_| helper_error("helper path is not a valid literal command"))?;
        let native = NativeLaunchDescription::new(
            command.clone(),
            helper_identity.clone(),
            manifest.canonical_bytes().to_vec(),
            manifest.digest(),
            manifest.preparation_digest(),
        )
        .and_then(|launch| launch.with_protected_handles(protected_handles))
        .map_err(|_| helper_error("C2 rejected the bounded Windows helper launch or handles"))?;
        let value = Self {
            command,
            helper_identity,
            handles: manifest.inherited_handles().clone(),
            token: manifest.token().clone(),
            job: manifest.job(),
            terminal: manifest.terminal(),
            manifest,
        };
        Ok((value, native))
    }

    /// Adds the protected C2 status and `ConPTY` resize channels to a native launch.
    #[cfg(target_os = "windows")]
    pub(crate) fn attach_helper_channels(
        native: NativeLaunchDescription,
        channels: peritus_process::NativeWindowsHelperChannels,
    ) -> Result<NativeLaunchDescription, WindowsError> {
        native
            .with_windows_helper_channels(channels)
            .map_err(|_| helper_error("C2 rejected Windows helper status/control channels"))
    }

    /// Returns literal helper command.
    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }
    /// Returns reviewed helper identity.
    #[must_use]
    pub fn helper_identity(&self) -> &str {
        &self.helper_identity
    }
    /// Returns complete helper manifest.
    #[must_use]
    pub const fn manifest(&self) -> &HelperManifest {
        &self.manifest
    }
    /// Returns exact inherited handle policy.
    #[must_use]
    pub const fn handles(&self) -> &InheritedHandlePolicy {
        &self.handles
    }
    /// Returns token/AppContainer selection.
    #[must_use]
    pub const fn token(&self) -> &TokenProfile {
        &self.token
    }
    /// Returns Job Object policy.
    #[must_use]
    pub const fn job(&self) -> JobPlan {
        self.job
    }
    /// Returns pipe/ConPTY mapping.
    #[must_use]
    pub const fn terminal(&self) -> TerminalMapping {
        self.terminal
    }
}

/// Reserved helper exits emitted only before activation succeeds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReservedHelperExit {
    /// Ready/manifest/checksum protocol failure.
    Protocol,
    /// Token/AppContainer activation failure.
    Token,
    /// Job Object/resource installation failure.
    JobOrResource,
    /// Protected inherited-handle failure.
    ProtectedHandle,
    /// Network isolation failure.
    Network,
    /// Secret staging failure.
    Secret,
    /// Literal target creation failure.
    TargetCreate,
    /// Helper executed outside Windows.
    UnsupportedPlatform,
}

/// Root termination classified without conflating helper failures and target status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelperExit {
    /// A reserved pre-target helper failure.
    Reserved(ReservedHelperExit),
    /// A numeric target exit.
    Target(i32),
    /// A status without a numeric code.
    NonNumeric,
}

impl HelperExit {
    /// Classifies a numeric root exit.
    #[must_use]
    pub const fn from_code(code: i32) -> Self {
        match ReservedHelperExit::from_code(code) {
            Some(value) => Self::Reserved(value),
            None => Self::Target(code),
        }
    }

    /// Classifies a root code only after C2 observed the protected target-started record.
    ///
    /// At this phase every numeric value belongs to the target, including values used by the
    /// helper before that record. The protected status channel, rather than an exit-code range,
    /// distinguishes pre-exec helper failures.
    #[must_use]
    pub const fn from_activated_code(code: i32) -> Self {
        Self::Target(code)
    }
}

impl ReservedHelperExit {
    /// Returns stable numeric category.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Protocol => 120,
            Self::Token => 121,
            Self::JobOrResource => 122,
            Self::ProtectedHandle => 123,
            Self::Network => 124,
            Self::Secret => 125,
            Self::TargetCreate => 126,
            Self::UnsupportedPlatform => 127,
        }
    }

    /// Classifies one reserved pre-activation exit.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            120 => Some(Self::Protocol),
            121 => Some(Self::Token),
            122 => Some(Self::JobOrResource),
            123 => Some(Self::ProtectedHandle),
            124 => Some(Self::Network),
            125 => Some(Self::Secret),
            126 => Some(Self::TargetCreate),
            127 => Some(Self::UnsupportedPlatform),
            _ => None,
        }
    }
}

/// Verifies protected channels and installs all manifest-bound Windows controls.
///
/// # Errors
/// Returns a typed fail-closed error if any required native control is unavailable or mismatched.
#[cfg(target_os = "windows")]
pub fn activate_manifest(manifest: &HelperManifest) -> Result<WindowsActivation, WindowsError> {
    crate::native::activate(manifest).map(|inner| WindowsActivation { inner })
}

/// Launches and waits for the literal target under the installed Windows controls.
///
/// # Errors
/// Returns a typed error if the literal target cannot be created or observed.
#[cfg(target_os = "windows")]
pub fn execute_manifest(
    manifest: &HelperManifest,
    activation: &WindowsActivation,
) -> Result<i32, WindowsError> {
    crate::native::execute(manifest, &activation.inner)
}

#[cfg(target_os = "windows")]
pub(crate) fn execute_manifest_with_channels(
    manifest: &HelperManifest,
    activation: &WindowsActivation,
    channels: &mut peritus_process::NativeWindowsHelperAttachment,
) -> Result<i32, WindowsError> {
    crate::native::execute_with_channels(manifest, &activation.inner, channels)
}

/// Returns strict unsupported behavior outside Windows.
///
/// # Errors
/// Always returns [`WindowsErrorKind::UnsupportedHost`] on non-Windows hosts.
#[cfg(not(target_os = "windows"))]
pub fn activate_manifest(_manifest: &HelperManifest) -> Result<WindowsActivation, WindowsError> {
    Err(crate::error::unsupported(
        WindowsOperation::Activate,
        "Windows controls cannot be activated on this platform",
    ))
}

/// Returns strict unsupported target execution outside Windows.
///
/// # Errors
/// Always returns [`WindowsErrorKind::UnsupportedHost`] on non-Windows hosts.
#[cfg(not(target_os = "windows"))]
pub fn execute_manifest(
    _manifest: &HelperManifest,
    _activation: &WindowsActivation,
) -> Result<i32, WindowsError> {
    Err(crate::error::unsupported(
        WindowsOperation::Activate,
        "Windows target execution is unavailable on this platform",
    ))
}

fn helper_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::HelperProtocol,
        WindowsOperation::Prepare,
        WindowsRecovery::RepairHelper,
        detail,
    )
}
