//! Exact staged-daemon effects for supported production resilience routes.

mod blob;
mod blob_corruption;
mod config;
mod daemon_lifecycle;
mod dependency;
mod disk;
mod evidence_corruption;
mod gate;
mod journal_after;
mod journal_before;
mod journal_corruption;
mod journal_disk;
mod lease;
mod observation;
mod patch;
mod process;
mod projection;
mod promotion;
mod promotion_evidence_corruption;
mod reboot;
mod snapshot;
mod snapshot_corruption;
mod snapshot_disk;

use peritus_approval::CredentialRegistrySnapshot;
use peritus_types::RevisionNumber;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use super::args::ControllerPaths;
use super::request::ScenarioRoute;
use config::{bytes_sha256, create_private_directory, render_configuration, write_new};
pub(super) use observation::{
    DaemonLifecycleCheckpointTruth, DaemonLifecycleObservation, DaemonLifecycleOwnership,
    DaemonLifecycleVerification, DependencyObservation, GateObservation, InjectedCandidate,
    LeaseObservation, PatchObservation, ProjectionCorruptionCheckpoint,
    ProjectionRepairObservation, PromotionCheckpoint, PromotionObservation, RecoveredCandidate,
    SnapshotObservation,
};
use process::{bounded_command, one_line};

pub(super) struct PreparedCandidate {
    pub(super) runtime: RuntimePaths,
    pub(super) journal_head_sha256: String,
    pub(super) version: String,
    reboot: Option<reboot::RebootRuntime>,
}

pub(super) struct RuntimePaths {
    root: PathBuf,
    state: PathBuf,
    config: PathBuf,
}

pub(super) fn prepare(
    paths: &ControllerPaths,
    route: ScenarioRoute,
) -> Result<PreparedCandidate, Box<dyn std::error::Error>> {
    let root = paths.subject_root.join("h1-controller-runtime");
    create_private_directory(&root)?;
    let state = root.join("state");
    create_private_directory(&state)?;
    let registry = root.join("approval-registry.bin");
    let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .map_err(|error| format!("construct H1 approval registry: {error:?}"))?;
    let registry_bytes = snapshot
        .canonical_bytes()
        .map_err(|error| format!("encode H1 approval registry: {error:?}"))?;
    write_new(&registry, &registry_bytes)?;
    let config = root.join("peritus.toml");
    let configuration = render_configuration(&state, &registry, &paths.build_sha256);
    write_new(&config, configuration.as_bytes())?;
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new("--version")],
        &root,
        &root.join("version.stdout"),
        &root.join("version.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd version probe failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let version = one_line(&output.stdout, "peritusd version")?;
    if !version.starts_with("peritusd ") {
        return Err("staged candidate returned an unknown version identity".into());
    }
    let mut baseline = Vec::with_capacity(configuration.len() + version.len() + 64);
    baseline.extend_from_slice(b"peritus/h1/journal-baseline/v1\0");
    baseline.extend_from_slice(configuration.as_bytes());
    baseline.extend_from_slice(version.as_bytes());
    let journal_head_sha256 = bytes_sha256(&baseline);
    let runtime = RuntimePaths { root, state, config };
    let reboot =
        route.reboot_phase().map(|phase| reboot::prepare(paths, &runtime, phase)).transpose()?;
    Ok(PreparedCandidate { runtime, journal_head_sha256, version, reboot })
}

pub(super) fn inject(
    paths: &ControllerPaths,
    prepared: &mut PreparedCandidate,
    route: ScenarioRoute,
    dependency_retry_limit: Option<u16>,
    request_sha256: &str,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    if let ScenarioRoute::HostReboot(phase) = route {
        return reboot::inject(
            paths,
            prepared.reboot.as_mut().ok_or("host reboot route has no disposable guest")?,
            phase,
            request_sha256,
        );
    }
    let runtime = &prepared.runtime;
    match route {
        ScenarioRoute::BlobBeforeDurableCommit | ScenarioRoute::BlobAfterDurableCommitBeforeAck => {
            blob::inject(paths, runtime, route)
        }
        ScenarioRoute::BlobCorruption => blob_corruption::inject(paths, runtime),
        ScenarioRoute::BlobFinalizeDiskExhaustion => disk::inject(paths, runtime),
        ScenarioRoute::JournalBeforeDurableCommit => journal_before::inject(paths, runtime),
        ScenarioRoute::JournalCorruption => journal_corruption::inject(paths, runtime),
        ScenarioRoute::AcceptanceEvidenceCorruption => evidence_corruption::inject(paths, runtime),
        ScenarioRoute::HarnessPromotionCorruption => {
            promotion_evidence_corruption::inject(paths, runtime)
        }
        ScenarioRoute::JournalAppendDiskExhaustion => journal_disk::inject(paths, runtime),
        ScenarioRoute::LeaseBeforeDurableCommit
        | ScenarioRoute::LeaseAfterDurableCommitBeforeAck => lease::inject(paths, runtime, route),
        ScenarioRoute::PatchBeforeDurableCommit
        | ScenarioRoute::PatchAfterDurableCommitBeforeAck => patch::inject(paths, runtime, route),
        ScenarioRoute::GateBeforeDurableCommit | ScenarioRoute::GateAfterDurableCommitBeforeAck => {
            gate::inject(paths, runtime, route)
        }
        ScenarioRoute::SnapshotBeforeDurableCommit
        | ScenarioRoute::SnapshotAfterDurableCommitBeforeAck => {
            snapshot::inject(paths, runtime, route)
        }
        ScenarioRoute::SnapshotCorruption => snapshot_corruption::inject(paths, runtime),
        ScenarioRoute::SnapshotCommitDiskExhaustion => snapshot_disk::inject(paths, runtime),
        ScenarioRoute::PromotionBeforeDurableCommit
        | ScenarioRoute::PromotionAfterDurableCommitBeforeAck => {
            promotion::inject(paths, runtime, route)
        }
        ScenarioRoute::ProjectionCorruption => projection::inject(paths, runtime),
        ScenarioRoute::JournalAfterDurableCommitBeforeAck => journal_after::inject(paths, runtime),
        ScenarioRoute::ProviderDeath
        | ScenarioRoute::ToolDeath
        | ScenarioRoute::WorkerDeath
        | ScenarioRoute::ProviderRetryExhaustion
        | ScenarioRoute::ToolRetryExhaustion
        | ScenarioRoute::WorkerRetryExhaustion => {
            dependency::inject(paths, runtime, route, dependency_retry_limit)
        }
        ScenarioRoute::DaemonLifecycle(_) => daemon_lifecycle::inject(paths, runtime, route),
        ScenarioRoute::HostReboot(_) => unreachable!(),
    }
}

pub(super) fn recover(
    paths: &ControllerPaths,
    prepared: &PreparedCandidate,
    injected: &InjectedCandidate,
    route: ScenarioRoute,
    dependency_retry_limit: Option<u16>,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    if let ScenarioRoute::HostReboot(phase) = route {
        return reboot::recover(
            paths,
            prepared.reboot.as_ref().ok_or("host reboot route has no disposable guest")?,
            injected,
            phase,
        );
    }
    let runtime = &prepared.runtime;
    match route {
        ScenarioRoute::BlobBeforeDurableCommit | ScenarioRoute::BlobAfterDurableCommitBeforeAck => {
            blob::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::BlobCorruption => blob_corruption::recover(paths, runtime, injected),
        ScenarioRoute::BlobFinalizeDiskExhaustion => disk::recover(paths, runtime, injected),
        ScenarioRoute::JournalBeforeDurableCommit => {
            journal_before::recover(paths, runtime, injected)
        }
        ScenarioRoute::JournalCorruption => journal_corruption::recover(paths, runtime, injected),
        ScenarioRoute::AcceptanceEvidenceCorruption => {
            evidence_corruption::recover(paths, runtime, injected)
        }
        ScenarioRoute::HarnessPromotionCorruption => {
            promotion_evidence_corruption::recover(paths, runtime, injected)
        }
        ScenarioRoute::JournalAppendDiskExhaustion => {
            journal_disk::recover(paths, runtime, injected)
        }
        ScenarioRoute::LeaseBeforeDurableCommit
        | ScenarioRoute::LeaseAfterDurableCommitBeforeAck => {
            lease::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::PatchBeforeDurableCommit
        | ScenarioRoute::PatchAfterDurableCommitBeforeAck => {
            patch::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::GateBeforeDurableCommit | ScenarioRoute::GateAfterDurableCommitBeforeAck => {
            gate::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::SnapshotBeforeDurableCommit
        | ScenarioRoute::SnapshotAfterDurableCommitBeforeAck => {
            snapshot::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::SnapshotCorruption => snapshot_corruption::recover(paths, runtime, injected),
        ScenarioRoute::SnapshotCommitDiskExhaustion => {
            snapshot_disk::recover(paths, runtime, injected)
        }
        ScenarioRoute::PromotionBeforeDurableCommit
        | ScenarioRoute::PromotionAfterDurableCommitBeforeAck => {
            promotion::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::ProjectionCorruption => projection::recover(paths, runtime, injected),
        ScenarioRoute::JournalAfterDurableCommitBeforeAck => {
            journal_after::recover(paths, runtime, injected)
        }
        ScenarioRoute::ProviderDeath
        | ScenarioRoute::ToolDeath
        | ScenarioRoute::WorkerDeath
        | ScenarioRoute::ProviderRetryExhaustion
        | ScenarioRoute::ToolRetryExhaustion
        | ScenarioRoute::WorkerRetryExhaustion => {
            dependency::recover(paths, runtime, injected, route, dependency_retry_limit)
        }
        ScenarioRoute::DaemonLifecycle(_) => {
            daemon_lifecycle::recover(paths, runtime, injected, route)
        }
        ScenarioRoute::HostReboot(_) => unreachable!(),
    }
}

pub(super) fn cleanup(prepared: &mut PreparedCandidate) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(reboot) = &mut prepared.reboot {
        reboot::cleanup(reboot)?;
    }
    let root = fs::canonicalize(&prepared.runtime.root)?;
    if root.parent().is_none() || root.file_name() != Some(OsStr::new("h1-controller-runtime")) {
        return Err("H1 controller refused an unexpected cleanup root".into());
    }
    fs::remove_dir_all(&root)?;
    if root.exists() {
        return Err("H1 controller runtime remained after cleanup".into());
    }
    Ok(())
}
