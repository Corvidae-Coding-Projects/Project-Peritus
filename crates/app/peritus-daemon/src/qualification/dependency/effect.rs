//! Direct provider, product-tool, and worker failure observations.

use std::path::Path;
use std::time::Duration;

use peritus_provider_core::{
    CancellationToken, ProcessExecutable, ProcessLimits, ProcessRequest, ProcessTransport,
    TokioProcessTransport,
};
use peritus_scheduler::SchedulerReservation;
use peritus_types::Sha256Digest;

use crate::worker::{
    WorkerFailureKind, WorkerShutdownDisposition, WorkerSupervisor, WorkerSupervisorLimits,
    WorkerTaskOutcome,
};
use crate::{DaemonConfig, DaemonError};

use super::{DependencyKind, dependency_error, receipt_path};

pub(super) struct EffectObservation {
    digest: Sha256Digest,
    receipt_bytes: u64,
    child_exit_code: Option<i32>,
}

impl EffectObservation {
    pub(super) const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    pub(super) const fn receipt_bytes(&self) -> u64 {
        self.receipt_bytes
    }
    pub(super) const fn child_exit_code(&self) -> Option<i32> {
        self.child_exit_code
    }
}

pub(super) async fn observe(
    config: &DaemonConfig,
    dependency: DependencyKind,
    attempt: u16,
    reservation: &SchedulerReservation,
    executable: &Path,
) -> Result<EffectObservation, DaemonError> {
    match dependency {
        DependencyKind::Provider => provider(config, dependency, attempt, executable).await,
        DependencyKind::Tool => tool(config, dependency, attempt, executable),
        DependencyKind::Worker => worker(reservation).await,
    }
}

async fn provider(
    config: &DaemonConfig,
    dependency: DependencyKind,
    attempt: u16,
    executable: &Path,
) -> Result<EffectObservation, DaemonError> {
    let executable = ProcessExecutable::pin(executable)
        .map_err(|error| dependency_error("pin provider router", error.to_string()))?;
    let limits = ProcessLimits::new(1_024, 4_096, 4_096, Duration::from_secs(10))
        .map_err(|error| dependency_error("bound provider router", error.to_string()))?;
    let request = ProcessRequest::new(
        executable,
        vec!["qualify-dependency-child".to_owned(), dependency.code().to_owned()],
        Vec::new(),
        Some(config.paths().state_root().to_path_buf()),
        Vec::new(),
        limits,
    )
    .map_err(|error| dependency_error("construct provider router request", error.to_string()))?;
    let output = TokioProcessTransport
        .run(request, &CancellationToken::new())
        .await
        .map_err(|error| dependency_error("run provider router", error.to_string()))?;
    if output.exit().success()
        || output.exit().code() != Some(17)
        || !output.stderr().is_empty()
        || !String::from_utf8_lossy(output.stdout()).contains("dependency-child")
    {
        return Err(dependency_error(
            "observe provider death",
            "provider router exit or bounded output differs",
        ));
    }
    Ok(EffectObservation {
        digest: effect_digest(dependency, attempt, output.stdout()),
        receipt_bytes: 0,
        child_exit_code: Some(17),
    })
}

fn tool(
    config: &DaemonConfig,
    dependency: DependencyKind,
    attempt: u16,
    executable: &Path,
) -> Result<EffectObservation, DaemonError> {
    let workspace = config.paths().state_root().join("qualification/tool-workspace");
    let observation = peritus_product_runner::qualification::qualify_tool_process_failure(
        &workspace,
        receipt_path(config, dependency, attempt),
        executable,
        dependency.code(),
        attempt,
    )
    .map_err(|error| dependency_error("run product tool", error.to_string()))?;
    Ok(EffectObservation {
        digest: observation.stdout_sha256(),
        receipt_bytes: observation.receipt_bytes(),
        child_exit_code: Some(observation.exit_code()),
    })
}

async fn worker(reservation: &SchedulerReservation) -> Result<EffectObservation, DaemonError> {
    let limits =
        WorkerSupervisorLimits::new(1, 1, 1, Duration::from_millis(10), Duration::from_secs(1))
            .map_err(|error| dependency_error("configure worker supervisor", error.to_string()))?;
    let mut supervisor = WorkerSupervisor::new(limits);
    supervisor
        .spawn_reserved(reservation, |_| async {
            std::future::pending::<WorkerTaskOutcome>().await
        })
        .map_err(|error| dependency_error("spawn owned worker", error.to_string()))?;
    tokio::task::yield_now().await;
    let report = supervisor.shutdown().await;
    let observation = report.observations().first().copied().ok_or_else(|| {
        dependency_error("observe worker death", "worker supervisor retained no terminal fact")
    })?;
    if report.disposition() != WorkerShutdownDisposition::Unclean
        || report.abort_requests() != 1
        || !report.remaining().is_empty()
        || observation.assignment().dispatch_id() != reservation.dispatch_id()
        || observation.outcome()
            != (WorkerTaskOutcome::Failed {
                kind: WorkerFailureKind::SupervisorAborted,
                evidence_digest: None,
            })
    {
        return Err(dependency_error(
            "observe worker death",
            "worker shutdown or exact ownership observation differs",
        ));
    }
    let mut bytes = Vec::with_capacity(80);
    bytes.extend_from_slice(b"peritus/h1/worker-death/v1\0");
    bytes.extend_from_slice(reservation.work_id().as_bytes());
    bytes.extend_from_slice(reservation.dispatch_id().as_bytes());
    bytes.extend_from_slice(reservation.worker_id().as_bytes());
    Ok(EffectObservation {
        digest: peritus_codec::sha256(&bytes),
        receipt_bytes: 0,
        child_exit_code: None,
    })
}

fn effect_digest(dependency: DependencyKind, attempt: u16, output: &[u8]) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(output.len() + 64);
    bytes.extend_from_slice(b"peritus/h1/dependency-effect/v1\0");
    bytes.extend_from_slice(dependency.code().as_bytes());
    bytes.extend_from_slice(&attempt.to_be_bytes());
    bytes.extend_from_slice(output);
    peritus_codec::sha256(&bytes)
}
