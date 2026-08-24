//! C2-owned process-group, PTY, and inherited-descriptor mapping.

use std::path::{Path, PathBuf};

use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, InputPermission, ResizePermission, SignalPolicy,
    TerminalMode, TerminalSignalPermission, TreeContainment,
};

use crate::{
    MacosError, MacosOperation, ManifestHandle, ProxyHandleDescriptor, SecretHandleDescriptor,
    error,
};

const MAX_INHERITED_DESCRIPTORS: usize = 64;

/// Complete helper and target containment requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "closed process containment facts are independently checked and encoded"
)]
pub struct ProcessContainment {
    new_process_group: bool,
    tree_required: bool,
    descendant_limit: u32,
    graceful_signal: bool,
    forced_signal: bool,
}

impl ProcessContainment {
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "decoder preserves the closed version-one field order"
    )]
    pub(crate) fn from_manifest(
        new_process_group: bool,
        tree_required: bool,
        descendant_limit: u32,
        graceful_signal: bool,
        forced_signal: bool,
    ) -> Result<Self, MacosError> {
        if !new_process_group || (forced_signal && !graceful_signal) {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "invalid process containment mapping",
            ));
        }
        Ok(Self {
            new_process_group,
            tree_required,
            descendant_limit,
            graceful_signal,
            forced_signal,
        })
    }

    /// Projects process ownership from a checked sandbox plan.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let contract = plan.contract().process();
        let descendant_limit = match contract.descendants() {
            DescendantPolicy::Denied => 0,
            DescendantPolicy::Bounded(limit) => limit,
        };
        let (graceful_signal, forced_signal) = match contract.signals() {
            SignalPolicy::Denied => (false, false),
            SignalPolicy::GracefulOnly => (true, false),
            SignalPolicy::GracefulAndForced => (true, true),
        };
        Self {
            new_process_group: true,
            tree_required: contract.containment() == TreeContainment::Required,
            descendant_limit,
            graceful_signal,
            forced_signal,
        }
    }

    /// Reports that C2 must place the helper and target in a fresh process group.
    #[must_use]
    pub const fn new_process_group(self) -> bool {
        self.new_process_group
    }

    /// Reports whether complete descendant containment is required.
    #[must_use]
    pub const fn tree_required(self) -> bool {
        self.tree_required
    }

    /// Returns the maximum permitted descendants.
    #[must_use]
    pub const fn descendant_limit(self) -> u32 {
        self.descendant_limit
    }

    /// Reports whether C2 may send graceful termination.
    #[must_use]
    pub const fn graceful_signal(self) -> bool {
        self.graceful_signal
    }

    /// Reports whether C2 may force termination.
    #[must_use]
    pub const fn forced_signal(self) -> bool {
        self.forced_signal
    }
}

/// Exact pipe or PTY ownership projected for the C2 supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalMapping {
    /// Separate C2-owned standard stream pipes.
    Pipes {
        /// Whether the checked target accepts standard input.
        input: bool,
    },
    /// One C2-owned controlling pseudoterminal.
    Pty {
        /// Initial columns.
        columns: u16,
        /// Initial rows.
        rows: u16,
        /// Whether C2 may resize the PTY.
        resize: bool,
        /// Whether C2 may forward terminal signals.
        signals: bool,
        /// Whether the checked target accepts terminal input.
        input: bool,
    },
}

impl TerminalMapping {
    /// Projects the exact terminal requirements from a checked plan.
    ///
    /// # Errors
    /// Returns a typed preparation error if checked PTY requirements lack dimensions.
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Result<Self, MacosError> {
        let terminal = plan.requirements().terminal();
        let input = terminal.input() == InputPermission::Allowed;
        match terminal.mode() {
            TerminalMode::Pipes => Ok(Self::Pipes { input }),
            TerminalMode::Pty => {
                let size = terminal.initial_size().ok_or_else(|| {
                    error::invalid(
                        MacosOperation::Prepare,
                        "checked PTY requirements lack dimensions",
                    )
                })?;
                Ok(Self::Pty {
                    columns: size.columns(),
                    rows: size.rows(),
                    resize: terminal.resize() == ResizePermission::Allowed,
                    signals: terminal.signals() == TerminalSignalPermission::Allowed,
                    input,
                })
            }
        }
    }
}

/// Role and destination number for one protected descriptor inherited by the helper.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InheritedDescriptor {
    /// Read-only helper manifest at the fixed protocol descriptor.
    Manifest(u32),
    /// Read-only opaque managed-proxy routing token descriptor.
    ProxyRouting(u32),
    /// Secret delivery handle whose bytes are absent from the manifest.
    Secret(u32),
}

impl InheritedDescriptor {
    /// Returns the destination descriptor number in the helper.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self {
            Self::Manifest(value) | Self::ProxyRouting(value) | Self::Secret(value) => value,
        }
    }
}

/// Structured direct-child launch description consumed only by the C2 process gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperLaunch {
    executable: PathBuf,
    arguments: Vec<String>,
    inherited_descriptors: Vec<InheritedDescriptor>,
    containment: ProcessContainment,
    terminal: TerminalMapping,
}

impl HelperLaunch {
    /// Creates a literal helper launch with no shell and no model-controlled arguments.
    ///
    /// # Errors
    /// Rejects a non-absolute helper path, descriptor collisions, or excessive handles.
    pub fn new(
        executable: PathBuf,
        manifest: ManifestHandle,
        proxy: Option<&ProxyHandleDescriptor>,
        secrets: &[SecretHandleDescriptor],
        containment: ProcessContainment,
        terminal: TerminalMapping,
    ) -> Result<Self, MacosError> {
        if !executable.is_absolute() || executable.as_os_str().is_empty() {
            return Err(error::invalid(
                MacosOperation::Prepare,
                "helper executable must be an absolute path",
            ));
        }
        let mut inherited_descriptors = vec![InheritedDescriptor::Manifest(manifest.descriptor())];
        if let Some(proxy) = proxy {
            inherited_descriptors
                .push(InheritedDescriptor::ProxyRouting(proxy.route().routing_handle()));
        }
        inherited_descriptors
            .extend(secrets.iter().map(|secret| InheritedDescriptor::Secret(secret.descriptor())));
        if inherited_descriptors.len() > MAX_INHERITED_DESCRIPTORS {
            return Err(error::limited(
                MacosOperation::Prepare,
                "too many inherited helper descriptors",
            ));
        }
        let mut numbers =
            inherited_descriptors.iter().map(|descriptor| descriptor.number()).collect::<Vec<_>>();
        if inherited_descriptors.iter().any(|descriptor| {
            !matches!(descriptor, InheritedDescriptor::Manifest(0)) && descriptor.number() < 3
        }) {
            return Err(error::invalid(
                MacosOperation::Prepare,
                "inherited descriptor overlaps standard streams",
            ));
        }
        numbers.sort_unstable();
        if numbers.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(error::invalid(
                MacosOperation::Prepare,
                "inherited helper descriptors collide",
            ));
        }
        Ok(Self { executable, arguments: Vec::new(), inherited_descriptors, containment, terminal })
    }

    /// Returns the exact helper executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns literal arguments. The production helper protocol uses none.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Returns the complete inherited-descriptor whitelist.
    #[must_use]
    pub fn inherited_descriptors(&self) -> &[InheritedDescriptor] {
        &self.inherited_descriptors
    }

    /// Returns C2 process-group ownership requirements.
    #[must_use]
    pub const fn containment(&self) -> ProcessContainment {
        self.containment
    }

    /// Returns C2 pipe or PTY ownership requirements.
    #[must_use]
    pub const fn terminal(&self) -> TerminalMapping {
        self.terminal
    }
}
