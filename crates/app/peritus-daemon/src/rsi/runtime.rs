//! Owned native delivery: real processes outside C0, exact settlement through the C0 owner.

use super::{
    evaluation as e,
    local::{LocalProcessBackend, LocalProcessCatalog, LocalProcessPort},
};
use crate::{
    AuthorityHandle, DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery,
    outbox::{DurableDelivery, DurableOutboxClaim, DurableOutboxPort},
};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_eval::{CancellationProbe, EvaluationPhase, RolloutId, RolloutRecord, RolloutStatus};
use peritus_process::ProcessStore;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(crate) struct NativeRsiPort {
    authority: AuthorityHandle,
    config: DaemonConfig,
    processes: ProcessStore,
    stopping: Arc<AtomicBool>,
}

impl NativeRsiPort {
    pub(crate) const fn new(
        authority: AuthorityHandle,
        config: DaemonConfig,
        processes: ProcessStore,
        stopping: Arc<AtomicBool>,
    ) -> Self {
        Self { authority, config, processes, stopping }
    }

    async fn execute(
        &self,
        claim: peritus_eval::ExecutionDirectiveClaim,
    ) -> Result<(), DaemonError> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(retry("native campaign dispatch is draining"));
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        let Some(work) =
            self.authority.begin_evaluation(claim.clone(), cancellation.clone()).await?
        else {
            return Ok(());
        };
        let campaign = work.spec.campaign_id();
        let cancelled = cancellation.clone();
        let stopping = self.stopping.clone();
        let authority = self.authority.clone();
        let (finished, mut done) = tokio::sync::watch::channel(false);
        let watcher = tokio::spawn(async move {
            loop {
                if *done.borrow() {
                    return Ok::<(), DaemonError>(());
                }
                if stopping.load(Ordering::Acquire) {
                    authority
                        .cancel_evaluation(
                            campaign,
                            peritus_codec::sha256(
                                b"native daemon shutdown cancelled owned candidate work",
                            ),
                        )
                        .await?;
                    cancelled.store(true, Ordering::Release);
                } else if authority
                    .evaluation(campaign)
                    .await?
                    .is_some_and(|s| s.phase() == EvaluationPhase::Cancelling)
                {
                    cancelled.store(true, Ordering::Release);
                }
                tokio::select! {
                    result = done.changed() => { if result.is_err() || *done.borrow() { return Ok(()); } },
                    () = tokio::time::sleep(Duration::from_millis(50)) => {},
                }
            }
        });
        let config = self.config.clone();
        let processes = self.processes.clone();
        let worker_claim = claim.clone();
        let stage_authority = self.authority.clone();
        let stage_runtime = tokio::runtime::Handle::current();
        let result = tokio::task::spawn_blocking(move || {
            let native = config.rsi().ok_or_else(|| {
                e::invalid(
                    "configure [rsi] native isolation components before running local candidates",
                )
            })?;
            let backend = LocalProcessBackend::new(
                native.bubblewrap.clone(),
                native.linux_helper.clone(),
                native.cgroup_root.clone(),
            )
            .map_err(e::error)?;
            let store = open_artifacts(&config)?;
            let catalog = LocalProcessCatalog::from_canonical_bytes(&e::read(
                &store,
                ArtifactDigest::from_sha256(work.profile.execution().image_digest()),
                16 * 1024 * 1024,
            )?)
            .map_err(e::error)?;
            let state_root = config.paths().state_root().join("rsi");
            let scratch = config.paths().workspace_root().join("rsi");
            std::fs::create_dir_all(&state_root).map_err(e::error)?;
            std::fs::create_dir_all(&scratch).map_err(e::error)?;
            let attempt = match work
                .committed
                .state()
                .rollout(work.spec.id())
                .map(peritus_eval::RolloutProgress::status)
            {
                Some(RolloutStatus::Running { attempt }) => attempt,
                _ => return Err(e::invalid("native work has no exact committed attempt")),
            };
            let retain_stage = |roots| {
                stage_runtime
                    .block_on(
                        stage_authority.retain_evaluation_artifacts(worker_claim.clone(), roots),
                    )
                    .map_err(|error| error.to_string())
            };
            let mut port = LocalProcessPort::new(
                &store,
                processes,
                &work.profile,
                &work.spec,
                attempt,
                &worker_claim,
                &work.committed,
                &catalog,
                &scratch,
                &state_root,
                &backend,
                cancellation.clone(),
                &retain_stage,
            )
            .map_err(e::error)?;
            let executed = peritus_eval::execute_rollout(
                &mut port,
                &Cancelled(cancellation),
                &work.spec,
                &work.profile,
                attempt,
            )
            .map_err(e::error)?;
            let mut roots = Vec::new();
            let candidate = port.candidate_artifacts();
            let evaluator = port.evaluator_artifacts();
            for stage in [candidate, evaluator].into_iter().flatten() {
                roots.extend([stage.stdout, stage.stderr, stage.terminal]);
            }
            roots.sort();
            roots.dedup();
            let manifest = serde_json::to_vec(&(
                1u16,
                *work.spec.id().as_bytes(),
                attempt,
                roots.iter().map(|d| *d.as_bytes()).collect::<Vec<_>>(),
            ))
            .map_err(e::error)?;
            let trace = e::save(&store, &manifest, work.committed.state().last_event_id())?;
            // The serialized result owner commits this exact physical manifest and its roots
            // as new immutable C0 dependencies; the worker never mutates an older batch roster.
            RolloutRecord::from_execution(&work.spec, executed, Some(trace), Some(trace))
                .map_err(e::error)
        })
        .await
        .map_err(e::error)
        .and_then(|r| r);
        let _ = finished.send(true);
        watcher.await.map_err(e::error)??;
        let record = result?;
        self.authority
            .finish_evaluation(
                claim,
                record,
                self.processes.clone(),
                self.config.paths().state_root().join("rsi"),
            )
            .await
    }
}

impl DurableOutboxPort for NativeRsiPort {
    fn deliver_and_settle(&self, claim: DurableOutboxClaim) -> DurableDelivery<'_> {
        Box::pin(async move {
            match claim {
                DurableOutboxClaim::EvaluationSchedule(claim) => {
                    let result = self.authority.deliver_evaluation_schedule(claim.clone()).await;
                    self.settle_failure(claim.into(), result).await
                }
                DurableOutboxClaim::EvaluationExecution(claim) => {
                    let result = self.execute(claim.clone()).await;
                    self.settle_failure(claim.into(), result).await
                }
                DurableOutboxClaim::EvaluationPublication(claim) => {
                    let result = self
                        .authority
                        .publish_evaluation(claim.clone(), self.config.paths().database())
                        .await;
                    self.settle_failure(claim.into(), result).await
                }
                DurableOutboxClaim::EvolutionPublication(claim) => {
                    self.authority
                        .publish_evolution(
                            claim,
                            self.config.paths().database(),
                            self.config.limits().maximum_artifact_bytes(),
                        )
                        .await
                }
                DurableOutboxClaim::DebuggerPublication(claim) => {
                    self.authority
                        .publish_debugger(
                            claim,
                            self.config.paths().database(),
                            self.config.paths().state_root().join("rsi"),
                            self.config.limits().maximum_artifact_bytes(),
                        )
                        .await
                }
                _ => Err(DaemonError::new(
                    DaemonErrorCode::Unsupported,
                    DaemonRecovery::Operator,
                    "native RSI delivery",
                    "this non-RSI destination requires its own production adapter",
                )),
            }
        })
    }
}

impl NativeRsiPort {
    async fn settle_failure(
        &self,
        claim: peritus_eval::EvaluationDirectiveClaim,
        result: Result<(), DaemonError>,
    ) -> Result<(), DaemonError> {
        match result {
            Ok(()) => Ok(()),
            Err(error) if error.recovery() == DaemonRecovery::Retry => Err(error),
            Err(error) => {
                // Missing/unrecoverable campaign inputs are a typed campaign failure; ordinary
                // daemon work remains writable. C0/claim settlement corruption still propagates.
                self.authority
                    .fail_evaluation(
                        claim,
                        error.to_string(),
                        self.processes.clone(),
                        self.config.clone(),
                    )
                    .await
            }
        }
    }
}
pub(crate) fn open_artifacts(config: &DaemonConfig) -> Result<ArtifactStore, DaemonError> {
    ArtifactStore::open(
        StoreConfig::new(
            config.paths().artifact_root(),
            config.limits().maximum_artifact_bytes(),
            config.limits().artifact_quota_bytes(),
        )
        .and_then(|c| c.with_database_path(config.paths().database()))
        .map_err(e::error)?,
    )
    .map_err(e::error)
}
struct Cancelled(Arc<AtomicBool>);
impl CancellationProbe for Cancelled {
    fn cancelled(&self, _rollout: RolloutId) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
fn retry(detail: &str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Worker,
        DaemonRecovery::Retry,
        "native RSI dispatch",
        detail.to_owned(),
    )
}
