//! Process-owned extension point for authorized native sandbox sessions.

mod observation;
mod protected_handle;
mod protocol;
#[cfg(unix)]
mod pty;
#[cfg(windows)]
mod windows_channel;

pub use protected_handle::NativeProtectedHandle;
pub use protocol::{
    native_activation_record, native_ready_record, native_target_exec_failed_record,
    native_target_started_record,
};
#[cfg(unix)]
pub use pty::{NATIVE_PTY_SLAVE_ENV, NativePtyAttachment};
#[cfg(windows)]
pub use windows_channel::{
    NATIVE_WINDOWS_CONTROL_HANDLE_ENV, NATIVE_WINDOWS_STATUS_HANDLE_ENV,
    NativeWindowsHelperAttachment, NativeWindowsHelperChannels,
};

use peritus_sandbox::{
    BackendAdmission, BackendDescriptor, CheckedSandboxPlan, EnforcementObservation,
};
use peritus_types::Sha256Digest;

use crate::{
    CancellationReason, CommandSpec, ErrorCode, ExecutionPlan, OsExitObservation, ProcessError,
    ProcessOperation, ProcessTreeIdentity, RecoveryClass,
};

pub(crate) use observation::{
    validate_activated_session, validate_prepared_session, validate_released_session,
    validate_terminated_session,
};

const MAX_HELPER_IDENTITY_BYTES: usize = 256;
const MAX_HELPER_MANIFEST_BYTES: usize = 4 * 1_024 * 1_024;

/// Native operating-system family implemented by a restricted backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativePlatform {
    /// Linux namespaces and kernel controls.
    Linux,
    /// macOS Seatbelt and process controls.
    Macos,
    /// Windows token, `AppContainer`, and job controls.
    Windows,
}

impl NativePlatform {
    /// Returns the platform on which this process crate was compiled.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            panic!("peritus-process supports only Linux, macOS, and Windows");
        }
    }
}

/// Structured direct-child command produced by one authorized native preparation.
#[derive(Clone, Debug)]
pub struct NativeLaunchDescription {
    command: CommandSpec,
    helper_identity: String,
    manifest: Vec<u8>,
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    protected_handles: Vec<NativeProtectedHandle>,
    #[cfg(windows)]
    windows_helper_channels: Option<NativeWindowsHelperChannels>,
}

impl NativeLaunchDescription {
    /// Creates a digest-bound helper launch description without a shell command line.
    ///
    /// # Errors
    ///
    /// Rejects an empty, oversized, non-ASCII, or control-bearing helper identity.
    pub fn new(
        command: CommandSpec,
        helper_identity: impl Into<String>,
        manifest: Vec<u8>,
        manifest_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
    ) -> Result<Self, ProcessError> {
        let helper_identity = helper_identity.into();
        if helper_identity.is_empty()
            || helper_identity.len() > MAX_HELPER_IDENTITY_BYTES
            || !helper_identity.is_ascii()
            || helper_identity.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(native_mismatch("native helper identity is invalid or exceeds its bound"));
        }
        if manifest.is_empty() || manifest.len() > MAX_HELPER_MANIFEST_BYTES {
            return Err(native_mismatch("native helper manifest is empty or exceeds its bound"));
        }
        if peritus_codec::sha256(&manifest) != manifest_digest {
            return Err(native_mismatch("native helper manifest digest does not match its bytes"));
        }
        Ok(Self {
            command,
            helper_identity,
            manifest,
            manifest_digest,
            preparation_digest,
            protected_handles: Vec::new(),
            #[cfg(windows)]
            windows_helper_channels: None,
        })
    }

    /// Adds the exact protected anonymous handles inherited by the native helper.
    ///
    /// Handle values remain unchanged across the helper launch so the backend manifest can bind
    /// each value to its exact proxy-token or secret-delivery destination. C2 retains ownership
    /// through the native session and enables inheritance only in the direct child.
    ///
    /// # Errors
    ///
    /// Rejects duplicate labels, duplicate operating-system handles, or an excessive set.
    pub fn with_protected_handles(
        mut self,
        mut handles: Vec<NativeProtectedHandle>,
    ) -> Result<Self, ProcessError> {
        const MAX_PROTECTED_HANDLES: usize = 256;
        if handles.len() > MAX_PROTECTED_HANDLES {
            return Err(native_mismatch("native protected handle count exceeds its bound"));
        }
        handles.sort_by(|left, right| left.label().cmp(right.label()));
        if handles.windows(2).any(|pair| pair[0].label() == pair[1].label()) {
            return Err(native_mismatch("native protected handle labels collide"));
        }
        let mut raw_handles =
            handles.iter().map(NativeProtectedHandle::raw_handle).collect::<Vec<_>>();
        raw_handles.sort_unstable();
        if raw_handles.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(native_mismatch("native protected operating-system handles collide"));
        }
        self.protected_handles = handles;
        Ok(self)
    }

    /// Adds C2-owned protected status and resize channels for the Windows helper.
    ///
    /// # Errors
    /// Rejects collisions with already staged protected handles.
    #[cfg(windows)]
    pub fn with_windows_helper_channels(
        mut self,
        mut channels: NativeWindowsHelperChannels,
    ) -> Result<Self, ProcessError> {
        let mut handles = core::mem::take(&mut self.protected_handles);
        handles.extend(channels.take_child_handles());
        self = self.with_protected_handles(handles)?;
        self.windows_helper_channels = Some(channels);
        Ok(self)
    }

    /// Returns the literal executable and argv used for the direct child.
    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }

    /// Returns the reviewed helper implementation identity.
    #[must_use]
    pub fn helper_identity(&self) -> &str {
        &self.helper_identity
    }

    /// Returns the bounded binary manifest written to the helper's inherited input stream.
    ///
    /// The helper consumes the length-prefixed frame before forwarding any subsequent target
    /// input. Secret values are never part of this manifest.
    #[must_use]
    pub fn manifest(&self) -> &[u8] {
        &self.manifest
    }

    /// Returns the digest of the bounded helper manifest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }

    /// Returns the admitted native preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }

    /// Returns the exact protected handles retained for the helper launch.
    #[must_use]
    pub fn protected_handles(&self) -> &[NativeProtectedHandle] {
        &self.protected_handles
    }

    /// Releases the parent-side ownership of one exact protected child handle.
    ///
    /// Returns whether a matching handle was present. Native sessions use this after the direct
    /// child has inherited a close-on-exec status writer, allowing the retained reader to observe
    /// target-exec success as EOF.
    pub fn release_protected_handle(&mut self, label: &str) -> bool {
        let Some(position) =
            self.protected_handles.iter().position(|handle| handle.label() == label)
        else {
            return false;
        };
        self.protected_handles.remove(position);
        true
    }

    #[cfg(windows)]
    pub(crate) const fn windows_helper_channels(&self) -> Option<&NativeWindowsHelperChannels> {
        self.windows_helper_channels.as_ref()
    }

    /// Returns the fixed record the helper writes after opening its protected channels.
    #[must_use]
    pub fn ready_record(&self) -> Sha256Digest {
        native_ready_record()
    }

    /// Returns the fixed record the helper writes after installing every admitted control.
    #[must_use]
    pub fn activation_record(&self) -> Sha256Digest {
        native_activation_record(self.manifest_digest, self.preparation_digest)
    }
}

/// Opaque post-consumption view passed only by [`crate::ExecutionGateway`].
///
/// Callers can inspect the exact plans while implementing a backend, but cannot construct this
/// value and therefore cannot invoke an authorized preparation independently.
pub struct AuthorizedPreparationContext<'a> {
    execution_plan: &'a ExecutionPlan,
    sandbox_plan: &'a CheckedSandboxPlan,
    admission: &'a BackendAdmission,
}

impl<'a> AuthorizedPreparationContext<'a> {
    pub(crate) const fn new(
        execution_plan: &'a ExecutionPlan,
        sandbox_plan: &'a CheckedSandboxPlan,
        admission: &'a BackendAdmission,
    ) -> Self {
        Self { execution_plan, sandbox_plan, admission }
    }

    /// Returns the exact authorized execution plan.
    #[must_use]
    pub const fn execution_plan(&self) -> &ExecutionPlan {
        self.execution_plan
    }

    /// Returns the exact checked sandbox plan.
    #[must_use]
    pub const fn sandbox_plan(&self) -> &CheckedSandboxPlan {
        self.sandbox_plan
    }

    /// Returns the exact admitted backend facts.
    #[must_use]
    pub const fn admission(&self) -> &BackendAdmission {
        self.admission
    }
}

/// Native backend called by the process gateway only after exact validation and durable consume.
pub trait NativeSandboxBackend: Send + 'static {
    /// Prepared session retained by the process supervisor through teardown.
    type Session: NativeSandboxSession;

    /// Returns the probed descriptor used by this implementation.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Returns the operating-system family this implementation enforces.
    fn platform(&self) -> NativePlatform;

    /// Prepares one session from the opaque authorized context.
    ///
    /// # Errors
    ///
    /// Returns a typed failure without starting the target when native preparation cannot be
    /// completed exactly.
    fn prepare(
        self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError>;
}

/// Native lifecycle state owned by the existing C2 supervisor.
pub trait NativeSandboxSession: Send + 'static {
    /// Returns the exact helper/direct-child launch description.
    fn launch_description(&self) -> &NativeLaunchDescription;

    /// Returns ordered, bounded, plan-bound native observations.
    fn observations(&self) -> &[EnforcementObservation];

    /// Polls backend-owned supervisor resource dimensions for the live tree.
    ///
    /// Hard-enforcement backends keep the default. Backends that truthfully advertise supervisor
    /// enforcement return [`NativePoll::ResourceLimitExceeded`] as soon as a checked ceiling is
    /// crossed; C2 then owns the ordinary first-trigger cancellation and reap path.
    ///
    /// # Errors
    /// Returns a typed fail-closed observation failure.
    fn poll_resources(&mut self, _tree: ProcessTreeIdentity) -> Result<NativePoll, ProcessError> {
        Ok(NativePoll::Continue)
    }

    /// Records successful target-tree activation.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError>;

    /// Records the first accepted cancellation request.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn cancellation_requested(&mut self, reason: CancellationReason) -> Result<(), ProcessError>;

    /// Records the observed root termination.
    ///
    /// # Errors
    /// Returns a typed fail-closed backend error.
    fn terminated(&mut self, exit: &OsExitObservation) -> Result<(), ProcessError>;

    /// Releases every backend-owned native, network, and secret resource.
    ///
    /// # Errors
    /// Returns a typed fail-closed error when complete release cannot be established.
    fn release(&mut self) -> Result<(), ProcessError>;
}

/// Result of one backend-owned supervisor resource sample.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativePoll {
    /// No checked native supervisor ceiling has been crossed.
    Continue,
    /// At least one checked native supervisor ceiling has been crossed.
    ResourceLimitExceeded,
}

const fn native_mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::PlanMismatch,
        ProcessOperation::Validate,
        RecoveryClass::SelectBackend,
        detail,
    )
}
