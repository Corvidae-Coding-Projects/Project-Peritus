//! Native C3 sandbox qualification contracts and observations.

use std::collections::BTreeSet;

use crate::{
    Platform, QualificationError, QualificationErrorCode, QualificationRecovery, Sha256Digest,
};

/// Native control whose availability or enforcement H2 observes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxCapability {
    /// Deny-by-default filesystem projection and protected metadata precedence.
    FilesystemIsolation,
    /// Complete owned process-tree containment and reap.
    ProcessTree,
    /// Cleared environment with exact declared restoration.
    EnvironmentIsolation,
    /// Deny-all network or exact managed-proxy egress.
    NetworkIsolation,
    /// Protected secret delivery with complete cleanup.
    SecretDelivery,
    /// Native or C2-supervised resource controls with truthful fidelity.
    ResourceControls,
    /// Pipes and platform terminal ownership.
    TerminalControls,
    /// Restart-safe native session reconciliation.
    Recovery,
}

/// Strength attributed to one observed capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EnforcementClaim {
    /// Enforced synchronously by a native kernel facility.
    NativeHard,
    /// Enforced by the owned C2 supervisor with a bounded observation cadence.
    SupervisorBounded,
    /// Not supported on the subject.
    Unsupported,
}

/// Closed native activation and release result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxExecutionResult {
    /// A restricted process used literal argv and every native/session resource was released.
    Qualified,
    /// The restricted plan was incorrectly routed through the raw C2 launcher.
    RawFallbackUsed,
    /// The helper or target command crossed a shell or otherwise lost literal argv.
    NonLiteralArguments,
    /// No directly executed restricted target was observed.
    RestrictedProcessNotObserved,
    /// At least one process, helper, proxy, secret, ACL/cgroup/profile, or task remained owned.
    ReleaseIncomplete,
}

/// Frozen H2 contract over the existing C2/C3 backend semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSandboxContract {
    platform: Platform,
    required: BTreeSet<SandboxCapability>,
    raw_fallback_permitted: bool,
    literal_argv_required: bool,
    complete_release_required: bool,
}

impl NativeSandboxContract {
    /// Returns the production contract for a target platform.
    #[must_use]
    pub fn production(platform: Platform) -> Self {
        Self {
            platform,
            required: [
                SandboxCapability::FilesystemIsolation,
                SandboxCapability::ProcessTree,
                SandboxCapability::EnvironmentIsolation,
                SandboxCapability::NetworkIsolation,
                SandboxCapability::SecretDelivery,
                SandboxCapability::ResourceControls,
                SandboxCapability::TerminalControls,
                SandboxCapability::Recovery,
            ]
            .into_iter()
            .collect(),
            raw_fallback_permitted: false,
            literal_argv_required: true,
            complete_release_required: true,
        }
    }

    /// Returns the target platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Borrows required capability domains.
    #[must_use]
    pub const fn required(&self) -> &BTreeSet<SandboxCapability> {
        &self.required
    }

    /// Reports whether a restricted plan may silently run through the raw C2 launcher.
    #[must_use]
    pub const fn raw_fallback_permitted(&self) -> bool {
        self.raw_fallback_permitted
    }

    /// Reports whether helper and target commands must remain structured argv.
    #[must_use]
    pub const fn literal_argv_required(&self) -> bool {
        self.literal_argv_required
    }

    /// Reports whether every helper, tree, proxy, secret, ACL/cgroup/profile, and support task must
    /// be released before a scenario passes.
    #[must_use]
    pub const fn complete_release_required(&self) -> bool {
        self.complete_release_required
    }

    /// Validates a native subject observation.
    ///
    /// # Errors
    ///
    /// Rejects target drift, missing controls, fallback, missing direct execution, or incomplete
    /// release.
    pub fn validate(&self, observation: &SandboxObservation) -> Result<(), QualificationError> {
        if observation.platform != self.platform {
            return Err(sandbox_error("sandbox observation platform differs from the contract"));
        }
        if observation.helper_digest != observation.manifest_helper_digest {
            return Err(sandbox_error("observed helper digest differs from the package manifest"));
        }
        if observation.probe_digest == Sha256Digest::new([0; 32]) {
            return Err(sandbox_error("native support probe digest is zero"));
        }
        if self.required.iter().any(|capability| {
            observation
                .claims
                .iter()
                .find(|(observed, _)| observed == capability)
                .is_none_or(|(_, claim)| *claim == EnforcementClaim::Unsupported)
        }) {
            return Err(sandbox_error("native backend does not enforce every required capability"));
        }
        if observation.execution != SandboxExecutionResult::Qualified {
            return Err(sandbox_error(
                "native sandbox activation, literal argv, no-fallback, or release evidence is incomplete",
            ));
        }
        Ok(())
    }
}

/// Bounded facts obtained by a target-native C3 qualification adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxObservation {
    platform: Platform,
    probe_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    manifest_helper_digest: Sha256Digest,
    claims: Vec<(SandboxCapability, EnforcementClaim)>,
    execution: SandboxExecutionResult,
}

impl SandboxObservation {
    /// Creates and validates a deterministic observation value.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or out-of-order capability entries.
    pub fn new(
        platform: Platform,
        probe_digest: Sha256Digest,
        helper_digest: Sha256Digest,
        manifest_helper_digest: Sha256Digest,
        claims: Vec<(SandboxCapability, EnforcementClaim)>,
        execution: SandboxExecutionResult,
    ) -> Result<Self, QualificationError> {
        if claims.is_empty()
            || claims.len() > 32
            || claims.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(sandbox_error(
                "sandbox capability observations must be nonempty, unique, and canonical",
            ));
        }
        Ok(Self {
            platform,
            probe_digest,
            helper_digest,
            manifest_helper_digest,
            claims,
            execution,
        })
    }

    /// Borrows canonical capability claims.
    #[must_use]
    pub fn claims(&self) -> &[(SandboxCapability, EnforcementClaim)] {
        &self.claims
    }

    /// Returns the native probe digest.
    #[must_use]
    pub const fn probe_digest(&self) -> Sha256Digest {
        self.probe_digest
    }

    /// Reports whether all native/session resources were released.
    #[must_use]
    pub const fn release_complete(&self) -> bool {
        matches!(self.execution, SandboxExecutionResult::Qualified)
    }

    /// Returns the typed activation and release result.
    #[must_use]
    pub const fn execution(&self) -> SandboxExecutionResult {
        self.execution
    }
}

fn sandbox_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Unsupported,
        QualificationRecovery::ConfigureHost,
        "validate native sandbox observation",
        detail,
    )
}
