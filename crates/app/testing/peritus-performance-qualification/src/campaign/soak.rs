//! Owned concurrent soak workers and observed cancellation/terminal collection.

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use peritus_benchmarks::{PlanKind, Workload};

use super::{CampaignRequest, WorkloadInvocation, WorkloadOutcome, micros, run_workload};
use crate::shared_accounting::SharedAccounting;
use crate::{CampaignError, CancellationFlag};

pub(super) fn run(
    soaks: Vec<Workload>,
    request: &CampaignRequest,
    campaign_origin: &Instant,
    cancellation: &CancellationFlag,
    accounting: &SharedAccounting,
) -> Result<Vec<WorkloadOutcome>, CampaignError> {
    let worker_count = soaks.len();
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_count);
    for workload in soaks {
        let sender = sender.clone();
        let invocation = WorkloadInvocation {
            subject: request.subject.clone(),
            implementation_revision: request.implementation_revision.clone(),
            run_id: request.run_id.clone(),
            runner: request.runner.clone(),
            profile: request.dataset.profile().clone(),
            workload,
            kind: PlanKind::Soak,
            elapsed_offset_micros: micros(campaign_origin.elapsed()),
            cancellation: cancellation.clone(),
            accounting: accounting.clone(),
        };
        let cancellation = cancellation.clone();
        handles.push(thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_workload(invocation)))
                    .unwrap_or(Err(CampaignError::WorkerPanicked));
            if !result.as_ref().is_ok_and(|outcome| outcome.receipt.completed()) {
                cancellation.cancel();
            }
            let _ = sender.send(result);
        }));
    }
    drop(sender);
    let mut outcomes = Vec::with_capacity(worker_count);
    let mut failure = None;
    for result in receiver {
        match result {
            Ok(outcome) => outcomes.push(outcome),
            Err(error) if failure.is_none() => failure = Some(error),
            Err(_) => {}
        }
    }
    if handles.into_iter().any(|handle| handle.join().is_err()) {
        return Err(CampaignError::WorkerPanicked);
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if outcomes.len() != worker_count {
        return Err(CampaignError::WorkerPanicked);
    }
    Ok(outcomes)
}
