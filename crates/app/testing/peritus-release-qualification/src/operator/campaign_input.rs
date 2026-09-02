//! Retained fresh-subject campaign admission through the standard H4 runner.

use std::collections::BTreeMap;

use peritus_release_artifacts::ReleaseBinding;

use crate::{
    CleanupObservation, CollectionRequest, CollectionRun, EvidenceKind, FreshSubjectFactory,
    FreshSubjectRunner, QualificationError, QualificationErrorCode, QualificationSubject,
    SignedEvidenceRecord, SubjectId,
};

use super::{OperatorError, admission::EvidenceStore, plan::CampaignSpec};

pub(super) fn assemble(
    binding: &ReleaseBinding,
    specs: &[CampaignSpec],
    evidence: &EvidenceStore,
) -> Result<CollectionRun, OperatorError> {
    let mut subjects = BTreeMap::new();
    for spec in specs {
        if spec.schema_version != 1 {
            return Err(OperatorError::integrity("campaign schema_version must be 1"));
        }
        if !EvidenceKind::fresh_subject_campaigns().contains(&spec.kind) {
            return Err(OperatorError::integrity(
                "campaign plan names a non-campaign evidence kind",
            ));
        }
        let retained: CampaignSpec = serde_json::from_slice(evidence.payload_for_kind(spec.kind)?)?;
        if retained != *spec {
            return Err(OperatorError::integrity(
                "campaign subject or cleanup facts differ from the signed payload",
            ));
        }
        let record = evidence.unique_kind(spec.kind)?.clone();
        let subject = RetainedSubject {
            id: SubjectId::new(&spec.subject_id)?,
            record,
            cleanup: CleanupObservation::new(
                spec.cleanup.remaining_processes,
                spec.cleanup.remaining_mounts,
                spec.cleanup.remaining_worktrees,
                spec.cleanup.remaining_temporary_paths,
            ),
        };
        if subjects.insert(spec.kind, subject).is_some() {
            return Err(OperatorError::integrity("campaign plan repeats an evidence kind"));
        }
    }
    let mut factory = RetainedFactory { subjects };
    Ok(FreshSubjectRunner::new(binding.clone()).run(&mut factory))
}

struct RetainedFactory {
    subjects: BTreeMap<EvidenceKind, RetainedSubject>,
}

impl FreshSubjectFactory for RetainedFactory {
    type Subject = RetainedSubject;

    fn create(&mut self, request: &CollectionRequest) -> Result<Self::Subject, QualificationError> {
        self.subjects.remove(&request.kind()).ok_or_else(|| {
            QualificationError::new(
                QualificationErrorCode::MissingEvidence,
                "admit retained H4 campaign",
                "campaign evidence is absent",
            )
        })
    }
}

struct RetainedSubject {
    id: SubjectId,
    record: SignedEvidenceRecord,
    cleanup: CleanupObservation,
}

impl QualificationSubject for RetainedSubject {
    fn subject_id(&self) -> &SubjectId {
        &self.id
    }

    fn collect(
        &mut self,
        _request: &CollectionRequest,
    ) -> Result<SignedEvidenceRecord, QualificationError> {
        Ok(self.record.clone())
    }

    fn close(self) -> Result<CleanupObservation, QualificationError> {
        Ok(self.cleanup)
    }
}
