//! A bound pending discard keeps its original authority across partial workspace changes.

use core::fmt::Write as _;

use super::{
    ProductRunServiceError, RunRecord, binding, failure, path, read_record, sync_directory,
};
use peritus_product_runner::{DiscardTransactionState, ProductRunner};
use peritus_types::{Sha256Digest, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write as _, path::Path};

const INTERRUPTION_CAUSE: &str = "explicit discard recovery is pending";

impl crate::product_run::ProductRunService {
    pub(in crate::product_run) fn ensure_workspace_available(
        &self,
        workspace: WorkspaceId,
    ) -> Result<(), ProductRunServiceError> {
        let records = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .clone();
        workspace_available(&self.inner.directory, &records, workspace)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::product_run) struct Pending {
    version: u8,
    binding: [u8; 32],
    plan: [u8; 32],
}

impl Pending {
    pub(in crate::product_run) fn identity(&self) -> String {
        let mut value = String::with_capacity(64);
        for byte in self.plan {
            let _ = write!(value, "{byte:02x}");
        }
        value
    }

    pub(in crate::product_run) fn read(
        directory: &Path,
        record: &RunRecord,
    ) -> Result<Option<Self>, ProductRunServiceError> {
        let Some(deliverable) = record.snapshot.deliverable() else { return Ok(None) };
        if deliverable.discarded() {
            return Ok(None);
        }
        let pending = path(directory, record).with_extension("discard-intent");
        let Some(bytes) = read_record(&pending)? else { return Ok(None) };
        let value: Self = serde_json::from_slice(&bytes).map_err(failure)?;
        if value.version != 1
            || value.binding != binding(record, deliverable)?
            || record.checkpoint.is_none()
            || record.task_baseline.is_none()
        {
            return Err(failure(
                "pending discard does not match its original candidate and task preimages",
            ));
        }
        Ok(Some(value))
    }

    pub(in crate::product_run) fn prepare(
        directory: &Path,
        record: &RunRecord,
        workspace: &Path,
    ) -> Result<Option<Self>, ProductRunServiceError> {
        if let Some(pending) = Self::read(directory, record)? {
            return Ok(Some(pending));
        }
        let (Some(baseline), Some(checkpoint)) = (&record.task_baseline, &record.checkpoint) else {
            return Ok(None);
        };
        let deliverable =
            record.snapshot.deliverable().ok_or(ProductRunServiceError::InvalidState)?;
        let binding = binding(record, deliverable)?;
        let plan = ProductRunner::prepare_discard_transaction(
            workspace,
            baseline,
            deliverable.changed_paths(),
            &path(directory, record).with_extension("discard-state"),
            Sha256Digest::new(binding),
            checkpoint.identity().repository_digest(),
        )
        .map_err(failure)?;
        let pending = Self { version: 1, binding, plan: plan.into_bytes() };
        let mut file = tempfile::NamedTempFile::new_in(directory).map_err(failure)?;
        file.write_all(&serde_json::to_vec(&pending).map_err(failure)?)
            .and_then(|()| file.as_file().sync_all())
            .map_err(failure)?;
        file.persist_noclobber(path(directory, record).with_extension("discard-intent"))
            .map_err(failure)?;
        sync_directory(directory)?;
        Ok(Some(pending))
    }

    pub(in crate::product_run) fn inspect(
        &self,
        directory: &Path,
        record: &RunRecord,
    ) -> Result<DiscardTransactionState, ProductRunServiceError> {
        ProductRunner::inspect_discard_transaction(
            &path(directory, record).with_extension("discard-state"),
            Sha256Digest::new(self.binding),
            Sha256Digest::new(self.plan),
        )
        .map_err(failure)?
        .ok_or_else(|| failure("pending discard lost its retained restore intent"))
    }

    pub(in crate::product_run) fn execute(
        &self,
        directory: &Path,
        record: &RunRecord,
    ) -> Result<Vec<std::path::PathBuf>, ProductRunServiceError> {
        ProductRunner::execute_discard_transaction(
            &path(directory, record).with_extension("discard-state"),
            Sha256Digest::new(self.binding),
            Sha256Digest::new(self.plan),
        )
        .map_err(failure)
    }

    pub(in crate::product_run) fn mark_interrupted(
        record: &mut RunRecord,
    ) -> Result<(), ProductRunServiceError> {
        record.snapshot = super::super::replace_snapshot(
            &record.snapshot,
            record.snapshot.phase(),
            "Discard interrupted; retry Discard explicitly",
            record.snapshot.summary(),
        )?;
        record.candidate_actionable = false;
        INTERRUPTION_CAUSE.clone_into(&mut record.interruption_cause);
        record.remaining_work =
            vec!["inspect the workspace and retry Discard for this run".to_owned()];
        Ok(())
    }

    pub(in crate::product_run) fn clear_interruption(record: &mut RunRecord) {
        if record.interruption_cause == INTERRUPTION_CAUSE {
            record.interruption_cause.clear();
            record.remaining_work.clear();
        }
    }
}

pub(in crate::product_run) fn workspace_available(
    directory: &Path,
    records: &BTreeMap<peritus_types::RunId, RunRecord>,
    workspace: WorkspaceId,
) -> Result<(), ProductRunServiceError> {
    for record in records.values().filter(|record| record.request.workspace_id() == workspace) {
        if Pending::read(directory, record)?.is_some() {
            return Err(ProductRunServiceError::internal(
                "resume interrupted discard",
                "Retry Discard for the original run before starting new workspace work",
            ));
        }
    }
    Ok(())
}
