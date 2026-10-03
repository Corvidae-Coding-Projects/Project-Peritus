//! Collect inert suggestions from terminal runs; inference starts only on explicit evaluation.

mod evaluation;
mod store;

use super::{ProductRunService, ProductRunServiceError as Error, RunRecord};
use peritus_app_protocol::{ImprovementInbox, ImprovementRequest, ProductRunPhase};
use peritus_types::{ActorId, WorkspaceId};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
pub(super) use store::Store;

impl ProductRunService {
    pub(crate) async fn improvements(
        &self,
        actor: ActorId,
        request: &ImprovementRequest,
    ) -> Result<ImprovementInbox, Error> {
        let workspace = request.workspace();
        if !self.inner.workspaces.contains_key(&workspace) {
            return Err(Error::WorkspaceUnavailable);
        }
        // Backfill terminal evidence after a crash without restarting any evaluation.
        self.collect_improvements(workspace)?;
        match request {
            ImprovementRequest::List(_) => {}
            ImprovementRequest::Suggest { run, proposal, .. } => {
                let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
                let record = records.get(run).ok_or(Error::NotFound)?;
                if record.request.workspace_id() != workspace || !record.snapshot.phase().terminal()
                {
                    return Err(Error::InvalidState);
                }
                locked(&self.inner.improvements)?.collect(
                    workspace,
                    *run,
                    proposal.as_str(),
                    &evidence(record),
                )?;
            }
            ImprovementRequest::Dismiss { candidate, .. } => {
                locked(&self.inner.improvements)?.dismiss(workspace, candidate.into_bytes())?;
            }
            ImprovementRequest::Evaluate { candidate, evaluation, .. } => {
                self.evaluate_improvement(actor, workspace, candidate.into_bytes(), *evaluation)
                    .await?;
            }
        }
        locked(&self.inner.improvements)?.inbox(workspace)
    }

    fn collect_improvements(&self, workspace: WorkspaceId) -> Result<(), Error> {
        let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
        let mut store = locked(&self.inner.improvements)?;
        let evaluating = store
            .inbox(workspace)?
            .candidates()
            .iter()
            .filter_map(peritus_app_protocol::ImprovementCandidate::evaluation)
            .map(peritus_app_protocol::ImprovementEvaluation::run)
            .collect::<Vec<_>>();
        for record in records.values().filter(|r| r.request.workspace_id() == workspace) {
            if !evaluating.contains(&record.request.run_id()) {
                match collect(&mut store, record) {
                    // A full inbox must remain readable and dismissible. The source run remains
                    // retained and can be collected on a later visit after the user makes room.
                    Ok(())
                    | Err(Error::Control(
                        peritus_product_runner::control::ControlError::Capacity,
                    )) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    pub(super) fn collect_improvement(&self, record: &RunRecord) -> Result<(), Error> {
        collect(&mut *locked(&self.inner.improvements)?, record)
    }
}

fn collect(store: &mut Store, record: &RunRecord) -> Result<(), Error> {
    let snapshot = &record.snapshot;
    if !snapshot.phase().terminal()
        || record.request.execution_task().starts_with("PERITUS HARNESS EVALUATION\n")
    {
        return Ok(());
    }
    let suggestion = if snapshot.phase() == ProductRunPhase::Failed {
        Some(
            "Investigate whether harness behavior contributed to these failed runs. Identify a reproducible cause in provider handling, tool execution, context, or orchestration; propose a regression-tested fix only if the evidence confirms a harness defect.",
        )
    } else if snapshot.cycle() >= 3 {
        Some(
            "Investigate repeated review/fix cycles. Look for a recurring harness instruction or verification gap, and propose a targeted change that reduces rework without weakening tests or independent review.",
        )
    } else {
        None
    };
    if let Some(proposal) = suggestion {
        store.collect(
            record.request.workspace_id(),
            record.request.run_id(),
            proposal,
            &evidence(record),
        )?;
    }
    Ok(())
}

fn evidence(record: &RunRecord) -> String {
    let s = &record.snapshot;
    let summary = format!(
        "Peritus {}; outcome {:?}; cycles {}\nStatus: {}\nSummary: {}\nChecks: {}\nReview: {}",
        env!("CARGO_PKG_VERSION"),
        s.phase(),
        s.cycle(),
        s.status(),
        s.summary(),
        s.gates(),
        s.review()
    );
    let mut value =
        summary.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).collect::<String>();
    if value.len() > 4096 {
        let mut end = 4093;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        value.push_str("...");
    }
    value
}

fn locked(store: &Mutex<Store>) -> Result<std::sync::MutexGuard<'_, Store>, Error> {
    store.lock().map_err(|_| Error::Unavailable)
}
fn digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}
