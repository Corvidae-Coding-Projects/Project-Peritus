//! C2-owned process-group, PTY, and inherited-descriptor mapping.

use std::path::{Path, PathBuf};

use peritus_process::{CommandSpec, NativeLaunchDescription, NativeProtectedHandle};
use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, InputPermission, ResizePermission, SignalPolicy,
    TerminalMode, TerminalSignalPermission, TreeContainment,
};

use crate::{
    EXEC_STATUS_LABEL, HelperManifest, MacosError, MacosErrorKind, MacosOperation,
    ManifestHandle, ProxyHandleDescriptor, RecoveryAction, SecretHandleDescriptor, error,
};

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
    unbounded_descendants: bool,
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
            unbounded_descendants: false,
            graceful_signal,
            forced_signal,
        })
    }

    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "decoder preserves the closed version-three field order"
    )]
    pub(crate) fn from_native_manifest(
        new_process_group: bool,
        tree_required: bool,
        descendant_limit: u32,
        unbounded_descendants: bool,
        graceful_signal: bool,
        forced_signal: bool,
    ) -> Result<Self, MacosError> {
        if unbounded_descendants && descendant_limit != 0 {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "unbounded descendant policy also contains a finite ceiling",
            ));
        }
        let mut containment = Self::from_manifest(
            new_process_group,
            tree_required,
            descendant_limit,
            graceful_signal,
            forced_signal,
        )?;
        containment.unbounded_descendants = unbounded_descendants;
        Ok(containment)
    }

    /// Projects process ownership from a checked sandbox plan.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan) -> Self {
        let contract = plan.contract().process();
        let (descendant_limit, unbounded_descendants) = match contract.descendants() {
            DescendantPolicy::Denied => (0, false),
            DescendantPolicy::Bounded(limit) => (limit, false),
            DescendantPolicy::Allowed => (0, true),
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
            unbounded_descendants,
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
    /// Close-on-exec target-status writer retained until literal target replacement.
    ExecutionStatus(u32),
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
            Self::Manifest(value)
            | Self::ExecutionStatus(value)
            | Self::ProxyRouting(value)
            | Self::Secret(value) => value,
        }
    }
}

/// Current operating-system authority for one complete helper descriptor contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelperDescriptorCapacity {
    required: u64,
    inherited_soft: u64,
    inherited_hard: u64,
}

impl HelperDescriptorCapacity {
    /// Returns the minimum descriptor slots required by standard protocol streams and the
    /// manifest/status/proxy/secret contract.
    #[must_use]
    pub const fn required(self) -> u64 {
        self.required
    }

    /// Returns the inherited soft descriptor ceiling observed during validation.
    #[must_use]
    pub const fn inherited_soft(self) -> u64 {
        self.inherited_soft
    }

    /// Returns the genuine operating-system hard descriptor authority.
    #[must_use]
    pub const fn inherited_hard(self) -> u64 {
        self.inherited_hard
    }
}

/// Structured direct-child launch description consumed only by the C2 process gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperLaunch {
    executable: PathBuf,
    arguments: Vec<String>,
    inherited_descriptors: Vec<InheritedDescriptor>,
    descriptor_capacity: HelperDescriptorCapacity,
    containment: ProcessContainment,
    terminal: TerminalMapping,
}

impl HelperLaunch {
    /// Creates a literal helper launch with no shell and no model-controlled arguments.
    ///
    /// # Errors
    /// Rejects a non-absolute helper path, descriptor collisions, or a contract beyond current
    /// operating-system descriptor authority.
    pub fn new(
        executable: PathBuf,
        manifest: ManifestHandle,
        execution_status: u32,
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
        let mut inherited_descriptors = vec![
            InheritedDescriptor::Manifest(manifest.descriptor()),
            InheritedDescriptor::ExecutionStatus(execution_status),
        ];
        if let Some(proxy) = proxy {
            inherited_descriptors
                .push(InheritedDescriptor::ProxyRouting(proxy.route().routing_handle()));
        }
        inherited_descriptors
            .extend(secrets.iter().map(|secret| InheritedDescriptor::Secret(secret.descriptor())));
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
        let required_slots = inherited_descriptors.len().checked_add(2).ok_or_else(|| {
            error::limited(
                MacosOperation::Prepare,
                "helper descriptor requirement cannot be represented",
            )
        })?;
        let descriptor_capacity = helper_descriptor_capacity(required_slots)?;
        if inherited_descriptors.iter().any(|descriptor| {
            u64::from(descriptor.number()) >= descriptor_capacity.inherited_hard()
        }) {
            return Err(error::limited(
                MacosOperation::Prepare,
                "an inherited helper descriptor exceeds operating-system hard authority",
            ));
        }
        Ok(Self {
            executable,
            arguments: Vec::new(),
            inherited_descriptors,
            descriptor_capacity,
            containment,
            terminal,
        })
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

    /// Returns the real platform capacity used to admit this complete descriptor contract.
    #[must_use]
    pub const fn descriptor_capacity(&self) -> HelperDescriptorCapacity {
        self.descriptor_capacity
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

    pub(crate) fn native_description(
        &self,
        helper_identity: String,
        manifest: &HelperManifest,
        protected_handles: Vec<NativeProtectedHandle>,
    ) -> Result<NativeLaunchDescription, MacosError> {
        if self.containment != manifest.containment()
            || self.terminal != manifest.terminal()
            || self.inherited_descriptors != descriptor_contract(manifest)
            || !protected_handle_owners_match(&protected_handles, manifest)
        {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "helper launch representation differs from its manifest or protected owners",
            ));
        }
        let command = CommandSpec::new(
            self.executable.as_os_str().to_owned(),
            self.arguments.iter().cloned(),
        )
        .map_err(native_launch_error)?;
        NativeLaunchDescription::new(
            command,
            helper_identity,
            manifest.canonical_bytes().to_vec(),
            manifest.digest(),
            manifest.preparation_digest(),
        )
        .and_then(|launch| launch.with_ordered_protected_handles(protected_handles))
        .map_err(native_launch_error)
    }
}

pub(crate) fn preflight_helper_descriptor_capacity(
    has_proxy: bool,
    secret_count: usize,
) -> Result<HelperDescriptorCapacity, MacosError> {
    let required = 4_usize
        .checked_add(usize::from(has_proxy))
        .and_then(|required| required.checked_add(secret_count))
        .ok_or_else(|| {
            error::limited(
                MacosOperation::Prepare,
                "helper descriptor requirement cannot be represented",
            )
        })?;
    helper_descriptor_capacity(required)
}

fn descriptor_contract(manifest: &HelperManifest) -> Vec<InheritedDescriptor> {
    let mut descriptors = vec![
        InheritedDescriptor::Manifest(ManifestHandle::protected_stdin().descriptor()),
        InheritedDescriptor::ExecutionStatus(manifest.exec_status_descriptor()),
    ];
    if let Some(proxy) = manifest.proxy_descriptor() {
        descriptors.push(InheritedDescriptor::ProxyRouting(
            proxy.route().routing_handle(),
        ));
    }
    descriptors.extend(
        manifest
            .secrets()
            .iter()
            .map(|secret| InheritedDescriptor::Secret(secret.descriptor())),
    );
    descriptors
}

pub(crate) fn protected_handle_owners_match(
    handles: &[NativeProtectedHandle],
    manifest: &HelperManifest,
) -> bool {
    let mut handles = handles.iter();
    let status_matches = handles.next().is_some_and(|handle| {
        handle.label() == EXEC_STATUS_LABEL
            && handle.raw_handle() == u64::from(manifest.exec_status_descriptor())
            && handle.payload_len().is_none()
    });
    let proxy_matches = manifest.proxy_descriptor().is_none_or(|proxy| {
        handles.next().is_some_and(|handle| {
            handle.label() == proxy.label()
                && handle.raw_handle() == u64::from(proxy.route().routing_handle())
                && handle.payload_len()
                    == Some(usize::try_from(proxy.payload_len()).unwrap_or(usize::MAX))
        })
    });
    status_matches
        && proxy_matches
        && manifest.secrets().iter().all(|secret| {
            handles.next().is_some_and(|handle| {
                handle.label() == secret.label()
                    && handle.raw_handle() == u64::from(secret.descriptor())
                    && handle.payload_len()
                        == Some(usize::try_from(secret.payload_len()).unwrap_or(usize::MAX))
            })
        })
        && handles.next().is_none()
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "getrlimit is the read-only Unix authority for helper descriptor capacity"
)]
fn helper_descriptor_capacity(required: usize) -> Result<HelperDescriptorCapacity, MacosError> {
    let mut inherited = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `inherited` is writable and getrlimit retains no caller-owned pointer.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut inherited) } != 0 {
        return Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::Prepare,
            RecoveryAction::SelectSupportedBackend,
            "operating-system descriptor authority cannot be queried",
        ));
    }
    let required = u64::try_from(required).map_err(|_| {
        error::limited(
            MacosOperation::Prepare,
            "helper descriptor requirement cannot be represented",
        )
    })?;
    let capacity = HelperDescriptorCapacity {
        required,
        inherited_soft: inherited.rlim_cur,
        inherited_hard: inherited.rlim_max,
    };
    if required > capacity.inherited_hard || capacity.inherited_hard == 0 {
        return Err(error::limited(
            MacosOperation::Prepare,
            "helper descriptor requirement exceeds operating-system hard authority",
        ));
    }
    Ok(capacity)
}

#[cfg(not(unix))]
fn helper_descriptor_capacity(_required: usize) -> Result<HelperDescriptorCapacity, MacosError> {
    Err(MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Prepare,
        RecoveryAction::SelectSupportedBackend,
        "macOS helper descriptor authority is unavailable on this platform",
    ))
}

fn native_launch_error(source: peritus_process::ProcessError) -> MacosError {
    MacosError::new(
        MacosErrorKind::PreparationMismatch,
        MacosOperation::Prepare,
        RecoveryAction::Reauthorize,
        "C2 rejected the native helper launch representation",
    )
    .with_source(error::process_source(&source))
}
