//! Collect inert suggestions from terminal runs; inference starts only on explicit evaluation.

mod evaluation;
mod context;
mod store;

use super::{ProductRunService, ProductRunServiceError as Error, RunRecord};
use peritus_app_protocol::{
    AppResponsePayload, ImprovementInbox, ImprovementRequest, ProductRunPhase,
};
use peritus_types::{ActorId, WorkspaceId};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
pub(super) use store::{StartupContention, Store, with_startup_cancellation};

impl ProductRunService {
    pub(crate) async fn improvements(
        &self,
        actor: ActorId,
        request: &ImprovementRequest,
    ) -> Result<ImprovementInbox, Error> {
        match self.improvements_with_representation(actor, request, false).await? {
            AppResponsePayload::Improvements(inbox) => Ok(inbox),
            _ => Err(Error::InvalidState),
        }
    }

    pub(crate) async fn improvements_with_representation(
        &self,
        actor: ActorId,
        request: &ImprovementRequest,
        paged_response: bool,
    ) -> Result<AppResponsePayload, Error> {
        let service = self.clone();
        let request = request.clone();
        Self::await_blocking_future("update durable improvement inbox", move || async move {
            service.improvements_owned(actor, request, paged_response).await
        })
        .await?
    }

    async fn improvements_owned(
        &self,
        actor: ActorId,
        request: ImprovementRequest,
        paged_response: bool,
    ) -> Result<AppResponsePayload, Error> {
        let workspace = request.workspace();
        if !self.inner.workspaces.contains_key(&workspace) {
            return Err(Error::WorkspaceUnavailable);
        }
        let mut mutation_candidate = None;
        match &request {
            ImprovementRequest::List(_) => {}
            ImprovementRequest::ListPage { after, .. } => {
                return Ok(AppResponsePayload::ImprovementPage(locked(&self.inner.improvements)?.page(workspace, *after)?));
            }
            ImprovementRequest::EvidencePage { candidate, revision, after, .. } => {
                return Ok(AppResponsePayload::ImprovementEvidencePage(locked(&self.inner.improvements)?.evidence_page(workspace, *candidate, *revision, *after)?));
            }
            ImprovementRequest::ReadText(query) => {
                return Ok(AppResponsePayload::ImprovementTextPage(locked(&self.inner.improvements)?.text_page(*query)?));
            }
            ImprovementRequest::Suggest { run, proposal, .. } => {
                let record = self
                    .inner
                    .records
                    .read()
                    .map_err(|_| Error::Unavailable)?
                    .get(run)
                    .cloned()
                    .ok_or(Error::NotFound)?;
                if record.request.workspace_id() != workspace || !record.snapshot.phase().terminal()
                    || record
                        .interaction
                        .persistence_failed
                        .load(std::sync::atomic::Ordering::Acquire)
                {
                    return Err(Error::InvalidState);
                }
                mutation_candidate = Some(locked(&self.inner.improvements)?.collect(
                    workspace,
                    *run,
                    proposal.as_str(),
                    &evidence(&record),
                )?);
            }
            ImprovementRequest::Dismiss { candidate, .. } => {
                locked(&self.inner.improvements)?.dismiss(workspace, candidate.into_bytes())?;
                mutation_candidate = Some(candidate.into_bytes());
            }
            ImprovementRequest::Evaluate { candidate, evaluation, .. } => {
                self.evaluate_improvement(actor, workspace, candidate.into_bytes(), *evaluation)
                    .await?;
                mutation_candidate = Some(candidate.into_bytes());
            }
        }
        if paged_response {
            let mut store = locked(&self.inner.improvements)?;
            Ok(AppResponsePayload::ImprovementPage(if let Some(candidate) = mutation_candidate {
                store.candidate_page(workspace, peritus_types::Sha256Digest::new(candidate))?
            } else {
                store.page(workspace, None)?
            }))
        } else {
            Ok(AppResponsePayload::Improvements(
                locked(&self.inner.improvements)?.inbox(workspace)?,
            ))
        }
    }

    pub(super) fn collect_improvement(&self, record: &RunRecord) -> Result<(), Error> {
        collect(&mut *locked(&self.inner.improvements)?, record)
    }
}

fn collect(store: &mut Store, record: &RunRecord) -> Result<(), Error> {
    let snapshot = &record.snapshot;
    if !snapshot.phase().terminal() {
        return Ok(());
    }
    let suggestion = if record.request.execution_task().starts_with("PERITUS HARNESS EVALUATION\n") {
        None
    } else if snapshot.phase() == ProductRunPhase::Failed {
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
    let summary = suggestion.map(|_| evidence(record));
    store.backfill_run(
        record.request.workspace_id(),
        record.request.run_id(),
        suggestion.zip(summary.as_deref()),
    )
}

pub(super) fn reconcile_backfill(
    store: &mut Store,
    records: &std::collections::BTreeMap<peritus_types::RunId, RunRecord>,
) -> Result<(), Error> {
    for record in records.values() {
        collect(store, record)?;
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
    summary.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).collect()
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
