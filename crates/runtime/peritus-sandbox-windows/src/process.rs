//! Restricted token, `AppContainer`, Job Object, terminal, and handle policy.

use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, InputPermission, ResizePermission, SignalPolicy,
    TerminalMode, TerminalSignalPermission, TreeContainment,
};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsOperation, error};

#[path = "process/profile.rs"]
mod profile;

/// Exact native token isolation selected for a target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenProfile {
    /// A restricted primary token with low mandatory integrity.
    RestrictedLowIntegrity {
        /// SID receiving exact temporary workspace grants.
        principal_sid: String,
    },
    /// An `AppContainer` with no ambient capabilities unless separately admitted.
    AppContainer(AppContainerProfile),
}

impl TokenProfile {
    /// Creates a restricted low-integrity profile for one exact SID.
    ///
    /// # Errors
    /// Rejects malformed SID text.
    pub fn restricted(principal_sid: impl Into<String>) -> Result<Self, WindowsError> {
        let principal_sid = principal_sid.into();
        validate_sid(&principal_sid)?;
        Ok(Self::RestrictedLowIntegrity { principal_sid })
    }

    /// Returns the exact principal that may receive temporary ACL grants.
    #[must_use]
    pub fn principal_sid(&self) -> &str {
        match self {
            Self::RestrictedLowIntegrity { principal_sid } => principal_sid,
            Self::AppContainer(profile) => profile.sid(),
        }
    }

    /// Reports whether this profile installs an `AppContainer` boundary.
    #[must_use]
    pub const fn is_app_container(&self) -> bool {
        matches!(self, Self::AppContainer(_))
    }
}

/// Exact installed `AppContainer` identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppContainerProfile {
    name: String,
    sid: String,
}

/// Desktop/console exposure selected for the target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DesktopPolicy {
    /// Pipe execution creates no visible console window.
    NonInteractive,
    /// The target inherits only the `ConPTY` already owned by C2.
    C2ConPty,
}

/// Exact kill-on-close Job Object projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobPlan {
    kill_on_close: bool,
    active_process_limit: Option<u32>,
    job_memory_bytes: Option<u64>,
    cpu_time_millis: Option<u64>,
}

impl JobPlan {
    /// Projects process containment and hard resource ceilings.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let limits = plan.requirements().resources();
        Self {
            kill_on_close: true,
            active_process_limit: plan.contract().process().maximum_process_limit(),
            job_memory_bytes: limits
                .selected_limit(peritus_sandbox::SandboxResourceKind::Memory)
                .map(peritus_types::ResourceQuantity::get),
            cpu_time_millis: limits
                .selected_limit(peritus_sandbox::SandboxResourceKind::CpuTime)
                .map(peritus_types::ResourceQuantity::get),
        }
    }

    pub(crate) fn from_manifest(
        kill_on_close: bool,
        active_process_limit: u32,
        job_memory_bytes: u64,
        cpu_time_millis: u64,
    ) -> Result<Self, WindowsError> {
        if !kill_on_close || active_process_limit == 0 || job_memory_bytes == 0 {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "legacy job policy is incomplete or has a zero hard ceiling",
            ));
        }
        let plan = Self {
            kill_on_close,
            active_process_limit: Some(active_process_limit),
            job_memory_bytes: Some(job_memory_bytes),
            cpu_time_millis: (cpu_time_millis != 0).then_some(cpu_time_millis),
        };
        plan.validate_native_capacity(WindowsOperation::Manifest)?;
        Ok(plan)
    }

    pub(crate) fn from_native_manifest(
        kill_on_close: bool,
        active_process_limit: Option<u32>,
        job_memory_bytes: Option<u64>,
        cpu_time_millis: Option<u64>,
    ) -> Result<Self, WindowsError> {
        if !kill_on_close
            || active_process_limit == Some(0)
            || job_memory_bytes == Some(0)
            || cpu_time_millis == Some(0)
        {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "native job policy has an invalid selected ceiling",
            ));
        }
        let plan = Self { kill_on_close, active_process_limit, job_memory_bytes, cpu_time_millis };
        plan.validate_native_capacity(WindowsOperation::Manifest)?;
        Ok(plan)
    }

    pub(crate) fn validate_native_capacity(
        self,
        operation: WindowsOperation,
    ) -> Result<(), WindowsError> {
        if let Some(bytes) = self.job_memory_bytes
            && usize::try_from(bytes).is_err()
        {
            return Err(WindowsError::new(
                crate::WindowsErrorKind::Resource,
                operation,
                crate::WindowsRecovery::CorrectRequest,
                "selected job-memory bytes exceed the native pointer-width representation",
            ));
        }
        if let Some(milliseconds) = self.cpu_time_millis
            && milliseconds
                .checked_mul(10_000)
                .and_then(|ticks| i64::try_from(ticks).ok())
                .is_none()
        {
            return Err(WindowsError::new(
                crate::WindowsErrorKind::Resource,
                operation,
                crate::WindowsRecovery::CorrectRequest,
                "selected CPU-time milliseconds exceed signed Windows 100ns job time",
            ));
        }
        Ok(())
    }

    /// Reports kill-on-close ownership.
    #[must_use]
    pub const fn kill_on_close(self) -> bool {
        self.kill_on_close
    }

    /// Returns the root-plus-descendants ceiling.
    #[must_use]
    pub const fn active_process_limit(self) -> u32 {
        match self.active_process_limit {
            Some(value) => value,
            None => 0,
        }
    }

    /// Returns the selected root-plus-descendants ceiling.
    #[must_use]
    pub const fn active_process_limit_option(self) -> Option<u32> {
        self.active_process_limit
    }

    /// Returns the Job Object memory ceiling.
    #[must_use]
    pub const fn job_memory_bytes(self) -> u64 {
        match self.job_memory_bytes {
            Some(value) => value,
            None => 0,
        }
    }

    /// Returns the selected Job Object memory ceiling.
    #[must_use]
    pub const fn job_memory_bytes_option(self) -> Option<u64> {
        self.job_memory_bytes
    }

    /// Returns the Job Object CPU-time ceiling.
    #[must_use]
    pub const fn cpu_time_millis(self) -> u64 {
        match self.cpu_time_millis {
            Some(value) => value,
            None => 0,
        }
    }

    /// Returns the selected Job Object CPU-time ceiling.
    #[must_use]
    pub const fn cpu_time_millis_option(self) -> Option<u64> {
        self.cpu_time_millis
    }
}

/// Exact C2-owned terminal mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalMapping {
    /// Separate inherited standard stream pipes.
    Pipes {
        /// Whether target input remains open after activation.
        input: bool,
    },
    /// The target inherits the `ConPTY` already owned by C2.
    ConPty {
        /// Initial columns.
        columns: u16,
        /// Initial rows.
        rows: u16,
        /// Whether C2 may resize the terminal.
        resize: bool,
        /// Whether C2 may send terminal controls.
        signals: bool,
        /// Whether target input remains open.
        input: bool,
    },
}

impl TerminalMapping {
    pub(crate) const fn pipes(input: bool) -> Self {
        Self::Pipes { input }
    }

    pub(crate) fn conpty(
        columns: u16,
        rows: u16,
        resize: bool,
        signals: bool,
        input: bool,
    ) -> Result<Self, WindowsError> {
        if columns == 0 || rows == 0 {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "ConPTY dimensions must be nonzero",
            ));
        }
        Ok(Self::ConPty { columns, rows, resize, signals, input })
    }

    /// Projects exact terminal requirements.
    ///
    /// # Errors
    /// Rejects PTY requirements without initial dimensions.
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Result<Self, WindowsError> {
        let terminal = plan.requirements().terminal();
        let input = terminal.input() == InputPermission::Allowed;
        match terminal.mode() {
            TerminalMode::Pipes => Ok(Self::Pipes { input }),
            TerminalMode::Pty => {
                let size = terminal.initial_size().ok_or_else(|| {
                    error::invalid(
                        WindowsOperation::Prepare,
                        "checked ConPTY requirements lack initial dimensions",
                    )
                })?;
                Ok(Self::ConPty {
                    columns: size.columns(),
                    rows: size.rows(),
                    resize: terminal.resize() == ResizePermission::Allowed,
                    signals: terminal.signals() == TerminalSignalPermission::Allowed,
                    input,
                })
            }
        }
    }

    /// Returns the corresponding desktop policy.
    #[must_use]
    pub const fn desktop(self) -> DesktopPolicy {
        match self {
            Self::Pipes { .. } => DesktopPolicy::NonInteractive,
            Self::ConPty { .. } => DesktopPolicy::C2ConPty,
        }
    }
}

/// Complete process-contract projection used by preparation and observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessPolicy {
    descendant_limit: u32,
    unbounded_descendants: bool,
    graceful: bool,
    forced: bool,
    tree_required: bool,
}

impl ProcessPolicy {
    pub(crate) fn from_manifest(
        descendant_limit: u32,
        graceful: bool,
        forced: bool,
        tree_required: bool,
    ) -> Result<Self, WindowsError> {
        if forced && !graceful {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "forced process control requires graceful control",
            ));
        }
        Ok(Self {
            descendant_limit,
            unbounded_descendants: false,
            graceful,
            forced,
            tree_required,
        })
    }

    pub(crate) fn from_native_manifest(
        descendant_limit: u32,
        unbounded_descendants: bool,
        graceful: bool,
        forced: bool,
        tree_required: bool,
    ) -> Result<Self, WindowsError> {
        if unbounded_descendants && descendant_limit != 0 {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "unbounded descendant policy also contains a finite ceiling",
            ));
        }
        let mut policy = Self::from_manifest(descendant_limit, graceful, forced, tree_required)?;
        policy.unbounded_descendants = unbounded_descendants;
        Ok(policy)
    }

    /// Projects exact checked process behavior.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let contract = plan.contract().process();
        let (descendant_limit, unbounded_descendants) = match contract.descendants() {
            DescendantPolicy::Denied => (0, false),
            DescendantPolicy::Bounded(value) => (value, false),
            DescendantPolicy::Allowed => (0, true),
        };
        let (graceful, forced) = match contract.signals() {
            SignalPolicy::Denied => (false, false),
            SignalPolicy::GracefulOnly => (true, false),
            SignalPolicy::GracefulAndForced => (true, true),
        };
        Self {
            descendant_limit,
            unbounded_descendants,
            graceful,
            forced,
            tree_required: contract.containment() == TreeContainment::Required,
        }
    }

    /// Returns the finite descendant count, or zero when denied or explicitly unbounded.
    #[must_use]
    pub const fn descendant_limit(self) -> u32 {
        self.descendant_limit
    }

    /// Reports whether descendants are allowed without an application-level count ceiling.
    #[must_use]
    pub const fn unbounded_descendants(self) -> bool {
        self.unbounded_descendants
    }

    /// Reports graceful-control authority.
    #[must_use]
    pub const fn graceful(self) -> bool {
        self.graceful
    }

    /// Reports forced-control authority.
    #[must_use]
    pub const fn forced(self) -> bool {
        self.forced
    }

    /// Reports whether complete tree containment is mandatory.
    #[must_use]
    pub const fn tree_required(self) -> bool {
        self.tree_required
    }
}

/// Closed handle whitelist bound into the helper manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InheritedHandlePolicy {
    handles: Vec<u64>,
    digest: Sha256Digest,
}

impl InheritedHandlePolicy {
    /// Canonicalizes the exact nonzero inherited handle set.
    ///
    /// # Errors
    /// Rejects null, duplicate, or excessive handles.
    pub fn new(mut handles: Vec<u64>) -> Result<Self, WindowsError> {
        Self::validate_count(handles.len())?;
        if handles.contains(&0) {
            return Err(error::invalid(
                WindowsOperation::Validate,
                "inherited handle whitelist contains a null handle",
            ));
        }
        handles.sort_unstable();
        if handles.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(error::invalid(
                WindowsOperation::Validate,
                "inherited handle whitelist contains a duplicate",
            ));
        }
        let mut bytes = Vec::with_capacity(handles.len() * 8);
        for handle in &handles {
            bytes.extend_from_slice(&handle.to_be_bytes());
        }
        let digest = peritus_codec::sha256(&bytes);
        Ok(Self { handles, digest })
    }

    pub(crate) fn validate_count(count: usize) -> Result<(), WindowsError> {
        count.checked_mul(core::mem::size_of::<usize>()).ok_or_else(|| {
            error::invalid(
                WindowsOperation::Validate,
                "selected inherited handle descriptor bytes exceed native representation",
            )
        })?;
        Ok(())
    }

    /// Returns the sorted exact handle whitelist.
    #[must_use]
    pub fn handles(&self) -> &[u64] {
        &self.handles
    }

    /// Returns the canonical whitelist digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn validate_sid(value: &str) -> Result<(), WindowsError> {
    let valid = value.strip_prefix("S-1-").is_some_and(|body| {
        let mut components = body.split('-');
        let authority = components.next().is_some_and(|component| {
            let parsed = component
                .strip_prefix("0x")
                .or_else(|| component.strip_prefix("0X"))
                .map_or_else(
                    || component.parse::<u64>(),
                    |hex| u64::from_str_radix(hex, 16),
                );
            parsed.is_ok_and(|authority| authority <= 0x0000_FFFF_FFFF_FFFF)
        });
        let mut subauthority_count = 0_usize;
        authority
            && components.all(|component| {
                subauthority_count += 1;
                subauthority_count <= 15 && component.parse::<u32>().is_ok()
            })
    });
    if valid {
        Ok(())
    } else {
        Err(error::invalid(WindowsOperation::Validate, "Windows SID text is malformed"))
    }
}
