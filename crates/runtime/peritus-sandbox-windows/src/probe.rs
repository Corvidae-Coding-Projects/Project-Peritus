//! Deterministic Windows capability probe and support intersection.

use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use peritus_sandbox::{CheckedSandboxPlan, FeatureSet, SandboxFeature};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{
    EnforcementLevel, ResourceControlPlan, TokenProfile, WindowsError, WindowsErrorKind,
    WindowsOperation, WindowsRecovery,
};

/// Minimum supported Windows build for 11 24H2 and Server 2025.
pub const MINIMUM_WINDOWS_BUILD: u32 = 26_100;

/// Raw bounded evidence from one Windows host probe.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "every independently probed native control remains explicit and digest-bound"
)]
pub struct ProbeEvidence {
    /// Windows build number reported by the kernel.
    pub os_build: Option<u32>,
    /// Whether compilation and runtime are Windows.
    pub platform: bool,
    /// Whether the architecture is x86-64 or `AArch64`.
    pub architecture: bool,
    /// Whether the exact helper is readable.
    pub helper: bool,
    /// Exact helper digest when readable.
    pub helper_digest: Option<Sha256Digest>,
    /// Restricted primary-token creation succeeded.
    pub restricted_token: bool,
    /// Low mandatory-integrity labeling is available.
    pub low_integrity: bool,
    /// Configured `AppContainer` identity can be derived exactly.
    pub app_container: bool,
    /// Configured `AppContainer` SID matches the derived SID.
    pub app_container_sid_exact: bool,
    /// A fresh Job Object can be created.
    pub job_object: bool,
    /// Kill-on-close limits can be installed.
    pub kill_on_close: bool,
    /// Exact ACL save/grant/restore tooling is available.
    pub acl: bool,
    /// Reparse-point and volume identities are observable.
    pub reparse: bool,
    /// Process attribute handle whitelisting is available.
    pub inherited_handle_list: bool,
    /// `ConPTY` APIs are present.
    pub conpty: bool,
    /// Windows Credential Manager APIs are present.
    pub credential_manager: bool,
    /// `AppContainer` deny-all network behavior is available.
    pub deny_network: bool,
    /// Dynamic WFP filter-to-managed-proxy enforcement is available.
    pub managed_network: bool,
    /// Dimension-specific hard/supervisor support.
    pub resources: [EnforcementLevel; 8],
}

impl ProbeEvidence {
    /// Returns complete deterministic evidence for platform-neutral contract tests.
    #[must_use]
    pub const fn supported_fixture() -> Self {
        Self {
            os_build: Some(MINIMUM_WINDOWS_BUILD),
            platform: true,
            architecture: true,
            helper: true,
            helper_digest: Some(Sha256Digest::new([0xC3; 32])),
            restricted_token: true,
            low_integrity: true,
            app_container: true,
            app_container_sid_exact: true,
            job_object: true,
            kill_on_close: true,
            acl: true,
            reparse: true,
            inherited_handle_list: true,
            conpty: true,
            credential_manager: true,
            deny_network: true,
            managed_network: true,
            resources: production_resource_levels(),
        }
    }

    /// Returns entirely unsupported evidence.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            os_build: None,
            platform: false,
            architecture: false,
            helper: false,
            helper_digest: None,
            restricted_token: false,
            low_integrity: false,
            app_container: false,
            app_container_sid_exact: false,
            job_object: false,
            kill_on_close: false,
            acl: false,
            reparse: false,
            inherited_handle_list: false,
            conpty: false,
            credential_manager: false,
            deny_network: false,
            managed_network: false,
            resources: [EnforcementLevel::Unsupported; 8],
        }
    }
}

/// Immutable probe request naming only installation/configuration facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeRequest {
    helper_path: PathBuf,
    acl_probe_root: Option<PathBuf>,
    token_profile: TokenProfile,
    managed_filter_digest: Option<Sha256Digest>,
}

impl ProbeRequest {
    /// Creates an exact bounded probe request.
    ///
    /// # Errors
    /// Rejects a non-absolute helper path or zero managed-filter digest.
    pub fn new(
        helper_path: PathBuf,
        token_profile: TokenProfile,
        managed_filter_digest: Option<Sha256Digest>,
    ) -> Result<Self, WindowsError> {
        if !helper_path.is_absolute() {
            return Err(probe_error("helper path must be absolute"));
        }
        if managed_filter_digest == Some(Sha256Digest::new([0; 32])) {
            return Err(probe_error("managed network filter digest cannot be zero"));
        }
        Ok(Self {
            helper_path,
            acl_probe_root: None,
            token_profile,
            managed_filter_digest,
        })
    }

    /// Selects a private absolute root for a reversible ACL save/grant/restore qualification.
    ///
    /// # Errors
    /// Rejects a relative probe root.
    pub fn with_acl_probe_root(
        mut self,
        acl_probe_root: PathBuf,
    ) -> Result<Self, WindowsError> {
        if !acl_probe_root.is_absolute() {
            return Err(probe_error("ACL capability probe root must be absolute"));
        }
        self.acl_probe_root = Some(acl_probe_root);
        Ok(self)
    }

    /// Returns the helper path.
    #[must_use]
    pub fn helper_path(&self) -> &Path {
        &self.helper_path
    }

    pub(crate) fn acl_probe_root(&self) -> Option<&Path> {
        self.acl_probe_root.as_deref()
    }

    /// Returns exact token/AppContainer configuration.
    #[must_use]
    pub const fn token_profile(&self) -> &TokenProfile {
        &self.token_profile
    }

    /// Returns configured managed-filter identity.
    #[must_use]
    pub const fn managed_filter_digest(&self) -> Option<Sha256Digest> {
        self.managed_filter_digest
    }
}

/// Canonical probe report and advertised feature intersection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsProbe {
    evidence: ProbeEvidence,
    features: FeatureSet,
    digest: Sha256Digest,
}

impl WindowsProbe {
    /// Validates and canonicalizes raw probe evidence.
    ///
    /// # Errors
    /// Rejects contradictory helper, `AppContainer`, job, or network evidence.
    pub fn from_evidence(evidence: ProbeEvidence) -> Result<Self, WindowsError> {
        if evidence.helper_digest.is_some() != evidence.helper
            || (evidence.app_container_sid_exact && !evidence.app_container)
            || (evidence.kill_on_close && !evidence.job_object)
            || (evidence.deny_network && !evidence.app_container_sid_exact)
            || (evidence.managed_network && !evidence.deny_network)
            || [1_usize, 2, 6].into_iter().any(|index| {
                evidence.resources[index] == EnforcementLevel::Hard && !evidence.job_object
            })
        {
            return Err(probe_error("Windows probe evidence is internally inconsistent"));
        }
        let features = supported_features(&evidence);
        let digest = peritus_codec::sha256(&probe_bytes(&evidence));
        Ok(Self { evidence, features, digest })
    }

    /// Executes bounded native probes on Windows and fails closed elsewhere.
    ///
    /// # Errors
    /// Returns a typed error only for inconsistent native observations.
    pub fn run(request: &ProbeRequest) -> Result<Self, WindowsError> {
        Self::run_cancellable(request, || true)
    }

    /// Executes native probes while the caller retains cancellation ownership.
    ///
    /// # Errors
    /// Returns a typed error for cancellation or inconsistent native observations.
    pub fn run_cancellable(
        request: &ProbeRequest,
        mut should_continue: impl FnMut() -> bool,
    ) -> Result<Self, WindowsError> {
        #[cfg(target_os = "windows")]
        let evidence = crate::native::probe::run(request, &mut should_continue)?;
        #[cfg(not(target_os = "windows"))]
        let evidence = {
            let _ = request;
            if should_continue() {
                ProbeEvidence::unsupported()
            } else {
                return Err(probe_cancelled());
            }
        };
        Self::from_evidence(evidence)
    }

    /// Returns raw bounded evidence.
    #[must_use]
    pub const fn evidence(&self) -> &ProbeEvidence {
        &self.evidence
    }

    /// Returns exactly supported C2 enforcement features.
    #[must_use]
    pub const fn supported_features(&self) -> FeatureSet {
        self.features
    }

    /// Returns deterministic full probe digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports whether core restricted execution is usable.
    #[must_use]
    pub fn core_supported(&self) -> bool {
        let evidence = &self.evidence;
        evidence.platform
            && evidence.architecture
            && evidence.helper
            && evidence.helper_digest.is_some()
            && evidence.restricted_token
            && evidence.low_integrity
    }

    pub(crate) fn selected_controls(
        &self,
        plan: &CheckedSandboxPlan,
        profile: &TokenProfile,
    ) -> Result<SelectedNativeControls, WindowsError> {
        if !self.core_supported()
            || !plan.required_features().is_subset_of(self.supported_features())
            || (plan.required_features().contains(SandboxFeature::NetworkDeny)
                && !profile.is_app_container())
        {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "selected Windows plan requires an unqualified native control",
            ));
        }
        let resources = ResourceControlPlan::select_checked_plan(plan, self.evidence.resources)?;
        let network = if plan.requirements().network().is_empty() {
            SelectedNetworkControl::DenyAll
        } else {
            SelectedNetworkControl::ManagedWfp
        };
        let terminal = if plan.required_features().contains(SandboxFeature::Pty) {
            SelectedTerminalControl::ConPty
        } else {
            SelectedTerminalControl::Pipes
        };
        Ok(SelectedNativeControls {
            network,
            terminal,
            credential_delivery: !plan.requirements().secrets().is_empty(),
            resources,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SelectedNativeControls {
    network: SelectedNetworkControl,
    terminal: SelectedTerminalControl,
    credential_delivery: bool,
    resources: ResourceControlPlan,
}

impl SelectedNativeControls {
    pub(crate) const fn managed_network(self) -> bool {
        matches!(self.network, SelectedNetworkControl::ManagedWfp)
    }

    pub(crate) const fn conpty(self) -> bool {
        matches!(self.terminal, SelectedTerminalControl::ConPty)
    }

    pub(crate) const fn credential_delivery(self) -> bool {
        self.credential_delivery
    }

    pub(crate) const fn resources(self) -> ResourceControlPlan {
        self.resources
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedNetworkControl {
    DenyAll,
    ManagedWfp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedTerminalControl {
    Pipes,
    ConPty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelperImageEvidence {
    digest: Sha256Digest,
    bytes: u64,
}

impl HelperImageEvidence {
    pub(crate) const fn digest(self) -> Sha256Digest {
        self.digest
    }

    pub(crate) const fn bytes(self) -> u64 {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelperImageFailure {
    Cancelled,
    Unavailable,
    Changed,
}

pub(crate) fn inspect_helper_image(
    path: &Path,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<HelperImageEvidence, HelperImageFailure> {
    let mut file = std::fs::File::open(path).map_err(|_| HelperImageFailure::Unavailable)?;
    let metadata = file.metadata().map_err(|_| HelperImageFailure::Unavailable)?;
    let expected = metadata.len();
    if !metadata.is_file() || expected == 0 {
        return Err(HelperImageFailure::Unavailable);
    }
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        if !should_continue() {
            return Err(HelperImageFailure::Cancelled);
        }
        let count = file.read(&mut buffer).map_err(|_| HelperImageFailure::Unavailable)?;
        if count == 0 {
            let final_length = file
                .metadata()
                .map_err(|_| HelperImageFailure::Unavailable)?
                .len();
            if bytes != expected || final_length != expected {
                return Err(HelperImageFailure::Changed);
            }
            return Ok(HelperImageEvidence {
                digest: Sha256Digest::new(digest.finalize().into()),
                bytes,
            });
        }
        bytes = bytes
            .checked_add(u64::try_from(count).map_err(|_| HelperImageFailure::Changed)?)
            .ok_or(HelperImageFailure::Changed)?;
        if bytes > expected {
            return Err(HelperImageFailure::Changed);
        }
        digest.update(&buffer[..count]);
    }
}

/// Expected Windows split between Job Object and C2 supervisor enforcement.
#[must_use]
pub const fn production_resource_levels() -> [EnforcementLevel; 8] {
    [
        EnforcementLevel::Supervisor,
        EnforcementLevel::Hard,
        EnforcementLevel::Hard,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Hard,
        EnforcementLevel::Supervisor,
    ]
}

fn supported_features(evidence: &ProbeEvidence) -> FeatureSet {
    let mut features = FeatureSet::empty();
    let baseline = evidence.platform
        && evidence.architecture
        && evidence.helper
        && evidence.helper_digest.is_some()
        && evidence.restricted_token
        && evidence.low_integrity;
    if !baseline {
        return features;
    }
    if evidence.acl && evidence.reparse {
        for feature in [
            SandboxFeature::FilesystemDiscover,
            SandboxFeature::FilesystemMetadata,
            SandboxFeature::FilesystemRead,
            SandboxFeature::FilesystemExecute,
            SandboxFeature::FilesystemCreate,
            SandboxFeature::FilesystemWrite,
            SandboxFeature::FilesystemRemove,
        ] {
            features.insert(feature);
        }
    }
    if evidence.job_object && evidence.kill_on_close && evidence.inherited_handle_list {
        for feature in [
            SandboxFeature::ProcessRoot,
            SandboxFeature::ProcessDescendants,
            SandboxFeature::ProcessSignals,
            SandboxFeature::ProcessTree,
        ] {
            features.insert(feature);
        }
    }
    for feature in [
        SandboxFeature::EnvironmentClear,
        SandboxFeature::EnvironmentAllowList,
    ] {
        features.insert(feature);
    }
    if evidence.inherited_handle_list {
        features.insert(SandboxFeature::Pipes);
        features.insert(SandboxFeature::Stdin);
    }
    if evidence.app_container && evidence.app_container_sid_exact && evidence.deny_network {
        features.insert(SandboxFeature::NetworkDeny);
    }
    for (index, feature) in RESOURCE_FEATURES.into_iter().enumerate() {
        if matches!(
            evidence.resources[index],
            EnforcementLevel::Hard | EnforcementLevel::Supervisor
        ) {
            features.insert(feature);
        }
    }
    if evidence.conpty && evidence.inherited_handle_list {
        features.insert(SandboxFeature::Pty);
        features.insert(SandboxFeature::TerminalResize);
        features.insert(SandboxFeature::TerminalSignals);
    }
    if evidence.managed_network
        && evidence.app_container
        && evidence.app_container_sid_exact
        && evidence.deny_network
    {
        features.insert(SandboxFeature::NetworkEgress);
    }
    if evidence.credential_manager && evidence.inherited_handle_list {
        features.insert(SandboxFeature::SecretEnvironment);
        features.insert(SandboxFeature::SecretFile);
        features.insert(SandboxFeature::SecretHandle);
    }
    features
}

const RESOURCE_FEATURES: [SandboxFeature; 8] = [
    SandboxFeature::WallTime,
    SandboxFeature::CpuTime,
    SandboxFeature::Memory,
    SandboxFeature::Disk,
    SandboxFeature::Output,
    SandboxFeature::OpenHandles,
    SandboxFeature::ProcessCount,
    SandboxFeature::Concurrency,
];

fn probe_bytes(evidence: &ProbeEvidence) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(b"PERITUS-WINDOWS-PROBE-V1\0");
    bytes.extend_from_slice(&evidence.os_build.unwrap_or(0).to_be_bytes());
    for fact in [
        evidence.platform,
        evidence.architecture,
        evidence.helper,
        evidence.restricted_token,
        evidence.low_integrity,
        evidence.app_container,
        evidence.app_container_sid_exact,
        evidence.job_object,
        evidence.kill_on_close,
        evidence.acl,
        evidence.reparse,
        evidence.inherited_handle_list,
        evidence.conpty,
        evidence.credential_manager,
        evidence.deny_network,
        evidence.managed_network,
    ] {
        bytes.push(u8::from(fact));
    }
    bytes
        .extend_from_slice(evidence.helper_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    bytes.extend(evidence.resources.iter().map(|level| level.ordinal()));
    bytes
}

fn probe_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::ProbeFailed,
        WindowsOperation::Probe,
        WindowsRecovery::ConfigureHost,
        detail,
    )
}

pub(crate) fn probe_cancelled() -> WindowsError {
    probe_error("native capability probe was cancelled by its caller")
}
