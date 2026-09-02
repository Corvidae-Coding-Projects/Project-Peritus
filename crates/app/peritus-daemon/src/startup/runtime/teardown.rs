//! Ordered shutdown runner with exact remaining-work accounting.

use std::time::Duration;

use peritus_app_protocol::{AppEventPayload, AppProtocolLimits};

use super::{DaemonRuntime, server_exit};
use crate::session::ShutdownCommand;
use crate::shutdown::{
    ShutdownBounds, ShutdownCoordinator, ShutdownOutcome, ShutdownStage, ShutdownWorkCounts,
};
use crate::startup::recovery::reconcile_processes;
use crate::worker::WorkerShutdownDisposition;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

impl DaemonRuntime {
    /// Closes intake and joins every owned runtime boundary within configured bounds.
    ///
    /// # Errors
    ///
    /// Returns the exact clean/unclean outcome after every cleanup boundary has been attempted.
    pub async fn shutdown(mut self) -> Result<ShutdownOutcome, DaemonError> {
        let mut coordinator = ShutdownCoordinator::begin(
            self.accepted_shutdown.as_ref().map(ShutdownCommand::request),
            ShutdownBounds::from_protocol(AppProtocolLimits::PRODUCTION)?,
        )?;
        let mut indeterminate_effects = 0;
        let _ = self.workers.begin_draining();
        let _ = self.server_stop.send(true);
        let timeout = Duration::from_millis(self.config.limits().shutdown_millis());
        if let Some(mut outbox) = self.outbox.take() {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                outbox.shutdown(timeout).await,
            )?;
        }
        self.product_runs.shutdown(timeout).await;
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            self.authority.begin_draining().await,
        )?;
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::AdmissionClosed,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )
        .await?;
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::ConnectionsDraining,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )
        .await?;
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::OutboxSettled,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )
        .await?;
        let worker_report = self.workers.shutdown().await;
        let worker_remaining = worker_report.remaining().len();
        indeterminate_effects =
            indeterminate_effects.saturating_add(worker_report.observations().len());
        if worker_report.disposition() != WorkerShutdownDisposition::Clean {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join supervised workers",
                    "worker tasks or terminal observations remain unsettled",
                )),
            )?;
        }
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::WorkersJoined,
            self.shutdown_counts(worker_remaining, 0, indeterminate_effects),
        )
        .await?;
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            self.terminals.shutdown().map(|_| ()).map_err(terminal_shutdown_error),
        )?;
        let process_reconciliation = match reconcile_processes(&self.processes) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(DaemonError::new(
                DaemonErrorCode::RecoveryRequired,
                DaemonRecovery::Reconcile,
                "reconcile processes during shutdown",
                "one or more process records remain indeterminate",
            )),
            Err(error) => Err(error),
        };
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            process_reconciliation,
        )?;
        if let Some(telemetry) = &mut self.telemetry {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                telemetry.shutdown(),
            )?;
        }
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::ProcessesReconciled,
            self.shutdown_counts(worker_remaining, 0, indeterminate_effects),
        )
        .await?;
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            self.authority.stop().await,
        )?;
        let authority_result = match tokio::time::timeout(timeout, &mut self.authority_task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(DaemonError::with_source(
                DaemonErrorCode::UncleanShutdown,
                DaemonRecovery::Reconcile,
                "join authority owner",
                "authority task panicked or was cancelled",
                error,
            )),
            Err(_) => {
                self.authority_task.abort();
                Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join authority owner",
                    "authority task exceeded the configured shutdown bound",
                ))
            }
        };
        retain_cleanup_failure(&mut coordinator, &mut indeterminate_effects, authority_result)?;
        let final_counts = self.shutdown_counts(worker_remaining, 0, indeterminate_effects);
        record_stage(
            &mut coordinator,
            self.accepted_shutdown.as_ref(),
            ShutdownStage::AuthorityStopped,
            final_counts,
        )
        .await?;
        let outcome = coordinator.complete(final_counts)?;
        if let (Some(delivery), Some(complete)) =
            (self.accepted_shutdown.as_ref(), outcome.protocol())
        {
            delivery.deliver(AppEventPayload::ShutdownComplete(complete.clone())).await;
        }
        if let Some(mut server_task) = self.server_task.take() {
            if let Ok(result) = tokio::time::timeout(timeout, &mut server_task).await {
                server_exit(result)?;
            } else {
                server_task.abort();
                let _ = server_task.await;
                return Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join local endpoint server",
                    "connection tasks exceeded the configured shutdown bound",
                ));
            }
        }
        Ok(outcome)
    }

    fn shutdown_counts(
        &self,
        workers: usize,
        outbox: usize,
        indeterminate_effects: usize,
    ) -> ShutdownWorkCounts {
        let (_, terminal_attachments) = self.terminals.counts();
        ShutdownWorkCounts::empty()
            .with_terminal_attachments(terminal_attachments)
            .with_workers(workers)
            .with_processes(self.processes.recovery_work_count())
            .with_outbox(outbox)
            .with_indeterminate_effects(indeterminate_effects)
    }
}

async fn record_stage(
    coordinator: &mut ShutdownCoordinator,
    delivery: Option<&ShutdownCommand>,
    stage: ShutdownStage,
    counts: ShutdownWorkCounts,
) -> Result<(), DaemonError> {
    if let Some(progress) = coordinator.record_stage(stage, counts)?
        && let Some(delivery) = delivery
    {
        delivery.deliver(AppEventPayload::ShutdownProgress(progress)).await;
    }
    Ok(())
}

fn terminal_shutdown_error(error: crate::terminal::TerminalBridgeError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::UncleanShutdown,
        DaemonRecovery::Reconcile,
        "shutdown terminal processes",
        "one or more owned terminal processes could not be cancelled and joined cleanly",
        error,
    )
}

fn retain_cleanup_failure(
    coordinator: &mut ShutdownCoordinator,
    indeterminate_effects: &mut usize,
    result: Result<(), DaemonError>,
) -> Result<(), DaemonError> {
    if let Err(error) = result {
        coordinator.record_failure(error.code_kind())?;
        *indeterminate_effects = indeterminate_effects.saturating_add(1);
    }
    Ok(())
}
