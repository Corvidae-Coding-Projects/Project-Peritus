//! Pure materialization plan construction.

use std::collections::BTreeSet;

use peritus_patch::{FileMode, Preimage, WorkspacePath};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::domain::HarnessRevision;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlan, MaterializationPlanId,
    MaterializationReason, MaterializationReceipt, MaterializationRecovery, ObservedTarget,
    PlannedFileOperation,
};

impl MaterializationPlan {
    /// Builds an exact path-sorted plan from a checked revision and C1 observation.
    ///
    /// # Errors
    /// Rejects stale prior receipts, unowned replacements, missing owned files, duplicate or
    /// colliding targets, empty patches, and configured byte or operation limits.
    pub fn build(
        command_id: CommandId,
        causal_event_id: EventId,
        revision: &HarnessRevision,
        target: ObservedTarget,
        reason: MaterializationReason,
        prior: Option<&MaterializationReceipt>,
    ) -> Result<Self, MaterializationError> {
        if let Some(prior) = prior
            && prior.after() != target.snapshot()
        {
            return Err(stale("prior receipt does not produce the target snapshot"));
        }
        let mut operations = Vec::new();
        let mut target_paths = BTreeSet::new();
        let mut total_bytes = 0_u64;
        for declaration in revision.graph().declarations() {
            let path = WorkspacePath::new(declaration.target_path().as_str().to_owned())
                .map_err(|_| invalid("checked target path is not representable by C1"))?;
            if !target_paths.insert(path.clone()) {
                return Err(invalid("checked revision repeats a target path"));
            }
            let preimage = target.preimage(&path);
            if !matches!(preimage, Preimage::Absent)
                && !prior.is_some_and(|receipt| receipt.owns_exact(&path, preimage))
            {
                return Err(MaterializationError::new(
                    MaterializationErrorKind::PathOwnership,
                    MaterializationRecovery::Reobserve,
                    "existing target is not owned by the exact prior receipt",
                ));
            }
            let byte_length = declaration.byte_length();
            total_bytes = total_bytes.checked_add(byte_length).ok_or_else(|| {
                MaterializationError::new(
                    MaterializationErrorKind::LimitExceeded,
                    MaterializationRecovery::CorrectInput,
                    "materialized byte total overflowed",
                )
            })?;
            operations.push(PlannedFileOperation::Install {
                path,
                preimage,
                artifact_digest: declaration.content_digest(),
                byte_length,
                mode: FileMode::Regular,
            });
        }
        if let Some(prior) = prior {
            for file in prior.files() {
                if target_paths.contains(file.path()) {
                    continue;
                }
                let observed = target.preimage(file.path());
                if observed != file.preimage() {
                    return Err(stale("owned deletion target differs from the prior receipt"));
                }
                operations.push(PlannedFileOperation::Delete {
                    path: file.path().clone(),
                    preimage: observed,
                });
            }
        }
        operations.sort_unstable_by(|left, right| left.path().cmp(right.path()));
        for pair in operations.windows(2) {
            if pair[0].path() == pair[1].path()
                || pair[1].path().as_str().starts_with(&format!("{}/", pair[0].path()))
            {
                return Err(invalid("materialization operations collide by target or ancestry"));
            }
        }
        if operations.is_empty() {
            return Err(invalid("materialization plan has no file operations"));
        }
        if total_bytes > revision.graph().limits().max_total_materialized_bytes() {
            return Err(MaterializationError::new(
                MaterializationErrorKind::LimitExceeded,
                MaterializationRecovery::CorrectInput,
                "materialization exceeds the revision byte limit",
            ));
        }
        let mut plan = Self {
            id: MaterializationPlanId::from_digest(Sha256Digest::new([0; 32])),
            digest: Sha256Digest::new([0; 32]),
            command_id,
            causal_event_id,
            harness_id: revision.harness_id(),
            revision_digest: revision.digest(),
            revision_number: revision.number().get(),
            graph_digest: revision.graph().graph_digest().digest(),
            target: target.snapshot,
            reason,
            prior_receipt: prior.map(MaterializationReceipt::id),
            operations,
            total_bytes,
        };
        plan.digest = peritus_codec::sha256(&plan.canonical_identity_bytes()?);
        plan.id = MaterializationPlanId::from_digest(plan.digest);
        Ok(plan)
    }
}

fn invalid(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::InvalidPlan,
        MaterializationRecovery::CorrectInput,
        detail,
    )
}

fn stale(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::StaleWorkspace,
        MaterializationRecovery::Reobserve,
        detail,
    )
}
