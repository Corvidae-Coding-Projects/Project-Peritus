//! Embed validated task preimages before publishing any managed candidate handoff.
use super::{ProductRunPhase, ProductRunService, launch, persist_record, replace_snapshot};
impl ProductRunService {
    pub(super) fn retain_task_baseline(&self, record: &mut crate::product_run::RunRecord) -> bool {
        if record.candidate_actionable && record.task_baseline_required {
            let trace = self
                .inner
                .directory
                .join(format!("{}.trace", launch::run_hex(record.request.run_id())));
            match peritus_product_runner::ProductRunner::retained_task_baseline(&trace) {
                Ok(Some(baseline)) => record.task_baseline = Some(baseline),
                missing => {
                    record.candidate_actionable = false;
                    let detail = match missing {
                        Err(error) => error.to_string(),
                        _ => "Task baseline is missing; user preimages cannot be verified."
                            .to_owned(),
                    };
                    if let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::RecoveryRequired,
                        "Retain task baseline failed",
                        &detail,
                    ) {
                        record.snapshot = snapshot;
                    }
                    let _ = persist_record(&self.inner.directory, record);
                    return false;
                }
            }
        }
        true
    }
}
