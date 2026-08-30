//! Restricted token, `AppContainer`, Job Object, terminal, and handle policy.

use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, InputPermission, ResizePermission, SignalPolicy,
    TerminalMode, TerminalSignalPermission, TreeContainment,
};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsOperation, error};

#[path = "process/profile.rs"]
mod profile;

const MAX_INHERITED_HANDLES: usize = 64;

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
    active_process_limit: u32,
    job_memory_bytes: u64,
    cpu_time_millis: u64,
}

impl JobPlan {
    /// Projects process containment and hard resource ceilings.
    #[must_use]
    pub const fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let limits = plan.requirements().resources();
        Self {
            kill_on_close: true,
            active_process_limit: plan.contract().process().maximum_processes(),
            job_memory_bytes: limits.limit(peritus_sandbox::SandboxResourceKind::Memory).get(),
            cpu_time_millis: limits.limit(peritus_sandbox::SandboxResourceKind::CpuTime).get(),
        }
    }

    pub(crate) fn from_manifest(
        kill_on_close: bool,
        active_process_limit: u32,
        job_memory_bytes: u64,
        cpu_time_millis: u64,
    ) -> Result<Self, WindowsError> {
        if !kill_on_close
            || active_process_limit == 0
            || job_memory_bytes == 0
            || cpu_time_millis == 0
        {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "job policy is incomplete or has a zero hard ceiling",
            ));
        }
        Ok(Self { kill_on_close, active_process_limit, job_memory_bytes, cpu_time_millis })
    }

    /// Reports kill-on-close ownership.
    #[must_use]
    pub const fn kill_on_close(self) -> bool {
        self.kill_on_close
    }

    /// Returns the root-plus-descendants ceiling.
    #[must_use]
    pub const fn active_process_limit(self) -> u32 {
        self.active_process_limit
    }

    /// Returns the Job Object memory ceiling.
    #[must_use]
    pub const fn job_memory_bytes(self) -> u64 {
        self.job_memory_bytes
    }

    /// Returns the Job Object CPU-time ceiling.
    #[must_use]
    pub const fn cpu_time_millis(self) -> u64 {
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
        Ok(Self { descendant_limit, graceful, forced, tree_required })
    }

    /// Projects exact checked process behavior.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let contract = plan.contract().process();
        let descendant_limit = match contract.descendants() {
            DescendantPolicy::Denied => 0,
            DescendantPolicy::Bounded(value) => value,
        };
        let (graceful, forced) = match contract.signals() {
            SignalPolicy::Denied => (false, false),
            SignalPolicy::GracefulOnly => (true, false),
            SignalPolicy::GracefulAndForced => (true, true),
        };
        Self {
            descendant_limit,
            graceful,
            forced,
            tree_required: contract.containment() == TreeContainment::Required,
        }
    }

    /// Returns the permitted descendant count.
    #[must_use]
    pub const fn descendant_limit(self) -> u32 {
        self.descendant_limit
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
        if handles.len() > MAX_INHERITED_HANDLES || handles.contains(&0) {
            return Err(error::invalid(
                WindowsOperation::Validate,
                "inherited handle whitelist is invalid or exceeds its bound",
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
    let valid = value.len() <= 184
        && value.starts_with("S-1-")
        && value.split('-').skip(2).all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        });
    if valid {
        Ok(())
    } else {
        Err(error::invalid(WindowsOperation::Validate, "Windows SID text is malformed"))
    }
}
