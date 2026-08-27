//! Installer, upgrade, rollback, and uninstall ownership plans.

use std::collections::BTreeSet;

use crate::{
    InstallPath, PackageManifest, PathOwnership, QualificationError, QualificationErrorCode,
    QualificationRecovery, ReleaseLayout,
};

/// Package lifecycle being planned.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LifecycleAction {
    /// First installation onto a clean subject.
    Install,
    /// Replace package-owned artifacts while preserving durable roots.
    Upgrade,
    /// Restore the immediately prior package-owned snapshot after a failed upgrade.
    Rollback,
    /// Remove package-owned artifacts while preserving configuration and state.
    Uninstall,
}

/// Owner whose data a step is permitted to mutate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlanOwnership {
    /// Staged package payload and installed package-owned files only.
    Package,
    /// Native per-user supervisor registration only.
    Supervisor,
    /// Runtime-owned endpoint and process lifecycle only.
    Runtime,
}

/// Result expected if an upgrade fails after its commit point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RollbackDisposition {
    /// No rollback is meaningful for this action.
    NotApplicable,
    /// Restore prior package files and prior service definition, retaining new diagnostic logs.
    RestorePriorPackage,
}

/// Closed mechanical operation performed by a native packaging adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleStep {
    /// Confirm target, architecture, release layout, and fresh-subject preconditions.
    ValidatePreconditions,
    /// Verify the canonical manifest and every staged artifact checksum before mutation.
    VerifyStagedArtifacts,
    /// Require an existing operator-provisioned strict G0 configuration.
    RequireOperatorConfiguration(InstallPath),
    /// Stop the supervisor and observe a bounded, terminal daemon exit.
    StopAndObserveDaemon,
    /// Snapshot only files currently owned by the installed package.
    SnapshotPackageFiles,
    /// Create one package-owned directory with its exact permission contract.
    CreatePackageDirectory(InstallPath),
    /// Install a staged file through same-directory write, permission, sync, and rename.
    PublishPackageFile(InstallPath),
    /// Apply the exact permission/ACL contract to an entry.
    ProtectInstalledEntry(InstallPath),
    /// Register or replace the native per-user supervisor definition.
    RegisterSupervisor(InstallPath),
    /// Remove the native supervisor registration before deleting its definition.
    UnregisterSupervisor,
    /// Start the foreground daemon under its native supervisor.
    StartDaemon,
    /// Await the exact G0 endpoint and require an authenticated `peritus status` response.
    AwaitAuthenticatedReadiness,
    /// Restore the package-owned pre-upgrade snapshot.
    RestorePackageSnapshot,
    /// Delete exactly one package-owned installed entry.
    RemovePackageEntry(InstallPath),
    /// Verify that operator/runtime-owned paths remain byte-for-byte outside package mutation.
    VerifyPreservedPath(InstallPath),
    /// Remove the bounded temporary package snapshot after successful readiness.
    RemovePackageSnapshot,
}

impl LifecycleStep {
    /// Returns the authority domain a step is permitted to affect.
    #[must_use]
    pub const fn ownership(&self) -> PlanOwnership {
        match self {
            Self::RegisterSupervisor(_) | Self::UnregisterSupervisor => PlanOwnership::Supervisor,
            Self::StopAndObserveDaemon | Self::StartDaemon | Self::AwaitAuthenticatedReadiness => {
                PlanOwnership::Runtime
            }
            Self::ValidatePreconditions
            | Self::VerifyStagedArtifacts
            | Self::RequireOperatorConfiguration(_)
            | Self::SnapshotPackageFiles
            | Self::CreatePackageDirectory(_)
            | Self::PublishPackageFile(_)
            | Self::ProtectInstalledEntry(_)
            | Self::RestorePackageSnapshot
            | Self::RemovePackageEntry(_)
            | Self::VerifyPreservedPath(_)
            | Self::RemovePackageSnapshot => PlanOwnership::Package,
        }
    }
}

/// Validated, ordered lifecycle and compensation plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecyclePlan {
    action: LifecycleAction,
    steps: Vec<LifecycleStep>,
    compensation: Vec<LifecycleStep>,
    preserved: Vec<InstallPath>,
    rollback: RollbackDisposition,
}

impl LifecyclePlan {
    /// Constructs the production plan for an exact layout and package manifest.
    ///
    /// # Errors
    ///
    /// Rejects a manifest/layout target mismatch or incomplete ownership contract.
    pub fn production(
        action: LifecycleAction,
        layout: &ReleaseLayout,
        manifest: &PackageManifest,
    ) -> Result<Self, QualificationError> {
        let platform_matches = manifest.platform() == layout.platform();
        let layout_matches = manifest.layout_digest() == layout.digest();
        if !platform_matches || !layout_matches {
            return Err(lifecycle_error(
                "package manifest platform or layout digest differs from the lifecycle target",
            ));
        }
        let package_entries = layout
            .entries()
            .iter()
            .filter(|entry| entry.ownership() == PathOwnership::Package)
            .map(|entry| entry.path().clone())
            .collect::<Vec<_>>();
        let mut preserved = layout
            .entries()
            .iter()
            .filter(|entry| entry.preserve_on_uninstall())
            .map(|entry| entry.path().clone())
            .collect::<Vec<_>>();
        preserved.sort();
        preserved.dedup();
        if preserved.is_empty()
            || !preserved.contains(layout.config_file())
            || !preserved.contains(layout.state_root())
        {
            return Err(lifecycle_error("configuration and state roots must be preserved"));
        }

        let mut steps =
            vec![LifecycleStep::ValidatePreconditions, LifecycleStep::VerifyStagedArtifacts];
        let mut compensation = Vec::new();
        let rollback = match action {
            LifecycleAction::Install => {
                steps.push(LifecycleStep::RequireOperatorConfiguration(
                    layout.config_file().clone(),
                ));
                publish_entries(&mut steps, layout, &package_entries);
                steps.push(LifecycleStep::RegisterSupervisor(layout.service_definition().clone()));
                steps.push(LifecycleStep::StartDaemon);
                steps.push(LifecycleStep::AwaitAuthenticatedReadiness);
                compensation.push(LifecycleStep::StopAndObserveDaemon);
                compensation.push(LifecycleStep::UnregisterSupervisor);
                for path in package_entries.iter().rev() {
                    compensation.push(LifecycleStep::RemovePackageEntry(path.clone()));
                }
                RollbackDisposition::NotApplicable
            }
            LifecycleAction::Upgrade => {
                steps.push(LifecycleStep::RequireOperatorConfiguration(
                    layout.config_file().clone(),
                ));
                steps.push(LifecycleStep::StopAndObserveDaemon);
                steps.push(LifecycleStep::SnapshotPackageFiles);
                publish_entries(&mut steps, layout, &package_entries);
                steps.push(LifecycleStep::RegisterSupervisor(layout.service_definition().clone()));
                steps.push(LifecycleStep::StartDaemon);
                steps.push(LifecycleStep::AwaitAuthenticatedReadiness);
                steps.extend(preserved.iter().cloned().map(LifecycleStep::VerifyPreservedPath));
                steps.push(LifecycleStep::RemovePackageSnapshot);
                compensation.extend([
                    LifecycleStep::StopAndObserveDaemon,
                    LifecycleStep::RestorePackageSnapshot,
                    LifecycleStep::RegisterSupervisor(layout.service_definition().clone()),
                    LifecycleStep::StartDaemon,
                    LifecycleStep::AwaitAuthenticatedReadiness,
                ]);
                RollbackDisposition::RestorePriorPackage
            }
            LifecycleAction::Rollback => {
                steps.push(LifecycleStep::StopAndObserveDaemon);
                steps.push(LifecycleStep::RestorePackageSnapshot);
                steps.push(LifecycleStep::RegisterSupervisor(layout.service_definition().clone()));
                steps.push(LifecycleStep::StartDaemon);
                steps.push(LifecycleStep::AwaitAuthenticatedReadiness);
                steps.extend(preserved.iter().cloned().map(LifecycleStep::VerifyPreservedPath));
                RollbackDisposition::RestorePriorPackage
            }
            LifecycleAction::Uninstall => {
                steps.push(LifecycleStep::StopAndObserveDaemon);
                steps.push(LifecycleStep::UnregisterSupervisor);
                for path in package_entries.iter().rev() {
                    steps.push(LifecycleStep::RemovePackageEntry(path.clone()));
                }
                steps.extend(preserved.iter().cloned().map(LifecycleStep::VerifyPreservedPath));
                RollbackDisposition::NotApplicable
            }
        };
        let unique_package = package_entries.iter().collect::<BTreeSet<_>>();
        if unique_package.len() != package_entries.len()
            || package_entries.iter().any(|path| preserved.contains(path))
        {
            return Err(lifecycle_error("package and preserved ownership sets overlap"));
        }
        Ok(Self { action, steps, compensation, preserved, rollback })
    }

    /// Returns the planned action.
    #[must_use]
    pub const fn action(&self) -> LifecycleAction {
        self.action
    }

    /// Borrows ordered forward steps.
    #[must_use]
    pub fn steps(&self) -> &[LifecycleStep] {
        &self.steps
    }

    /// Borrows ordered compensation steps executed after forward failure.
    #[must_use]
    pub fn compensation(&self) -> &[LifecycleStep] {
        &self.compensation
    }

    /// Borrows configuration/state/log paths protected from package deletion.
    #[must_use]
    pub fn preserved_paths(&self) -> &[InstallPath] {
        &self.preserved
    }

    /// Returns the rollback disposition.
    #[must_use]
    pub const fn rollback_disposition(&self) -> RollbackDisposition {
        self.rollback
    }
}

fn publish_entries(
    steps: &mut Vec<LifecycleStep>,
    layout: &ReleaseLayout,
    entries: &[InstallPath],
) {
    for path in entries {
        if let Some(entry) = layout.entries().iter().find(|entry| entry.path() == path) {
            if entry.kind() == crate::EntryKind::Directory {
                steps.push(LifecycleStep::CreatePackageDirectory(path.clone()));
            } else {
                steps.push(LifecycleStep::PublishPackageFile(path.clone()));
                steps.push(LifecycleStep::ProtectInstalledEntry(path.clone()));
            }
        }
    }
}

fn lifecycle_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Lifecycle,
        QualificationRecovery::RebuildRelease,
        "construct package lifecycle plan",
        detail,
    )
}
