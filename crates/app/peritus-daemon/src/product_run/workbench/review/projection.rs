//! Typed review anchors, comment state, and independent qualification evidence.

use super::{
    ControlError, ControlOperationId, Error, EvidenceStatus, QualificationEvidence, ReviewAnchor,
    ReviewCommentState, ReviewFeedback, ReviewTarget, WorkbenchDiffFile, WorkbenchInputId,
    WorkbenchInputSelection, WorkbenchInputText, WorkbenchReviewAnchor, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewRange,
    WorkbenchReviewTarget, contains,
};

pub(super) fn project_anchor(value: &ReviewAnchor) -> Result<WorkbenchReviewAnchor, Error> {
    let path = value.path().to_str().ok_or(ControlError::InvalidInput)?.to_owned();
    let target = match value.target() {
        ReviewTarget::File => WorkbenchReviewTarget::File,
        ReviewTarget::Hunk => WorkbenchReviewTarget::Hunk,
    };
    let range = match target {
        WorkbenchReviewTarget::File => WorkbenchReviewRange::file(),
        WorkbenchReviewTarget::Hunk => WorkbenchReviewRange::hunk(
            value.range().old_start(),
            value.range().old_lines(),
            value.range().new_start(),
            value.range().new_lines(),
        )
        .map_err(|_| ControlError::InvalidInput)?,
    };
    WorkbenchReviewAnchor::new(
        value.run(),
        value.workspace(),
        value.candidate_digest(),
        value.diff_digest(),
        path,
        value.before_blob_digest(),
        value.after_blob_digest(),
        value.context_digest(),
        target,
        range,
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn project_comment(
    value: &peritus_product_runner::control::ReviewComment,
    files: &[WorkbenchDiffFile],
) -> Result<WorkbenchReviewComment, Error> {
    let anchor = project_anchor(value.anchor())?;
    let state = if value.state() == ReviewCommentState::Dismissed {
        WorkbenchReviewCommentState::Dismissed
    } else if !contains(files, &anchor) {
        WorkbenchReviewCommentState::Stale
    } else if value.state() == ReviewCommentState::Addressed {
        WorkbenchReviewCommentState::Addressed
    } else {
        WorkbenchReviewCommentState::Open
    };
    let feedback = match value.feedback() {
        ReviewFeedback::Explain => WorkbenchReviewFeedback::Explain,
        ReviewFeedback::RequestRevision => WorkbenchReviewFeedback::RequestRevision,
        ReviewFeedback::KeepBehavior => WorkbenchReviewFeedback::KeepBehavior,
        ReviewFeedback::LeaveAlone => WorkbenchReviewFeedback::LeaveAlone,
    };
    let id =
        ControlOperationId::new(*value.id().as_bytes()).map_err(|_| ControlError::InvalidInput)?;
    let input_id = WorkbenchInputId::new(*value.input().id().as_bytes())
        .map_err(|_| ControlError::InvalidInput)?;
    let input = WorkbenchInputSelection::new(input_id, value.input().revision())
        .map_err(|_| ControlError::InvalidInput)?;
    let message = WorkbenchInputText::new(value.message().to_owned())
        .map_err(|_| ControlError::InvalidInput)?;
    WorkbenchReviewComment::new(id, value.revision(), anchor, feedback, message, input, state)
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn missing(kind: WorkbenchReviewEvidenceKind) -> Result<WorkbenchReviewEvidence, Error> {
    WorkbenchReviewEvidence::new(kind, WorkbenchReviewEvidenceState::Missing, None, None, None)
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn project_evidence(
    kind: WorkbenchReviewEvidenceKind,
    value: &EvidenceStatus<QualificationEvidence>,
) -> Result<WorkbenchReviewEvidence, Error> {
    let state = match value {
        EvidenceStatus::Missing => WorkbenchReviewEvidenceState::Missing,
        EvidenceStatus::Current(_) => WorkbenchReviewEvidenceState::Current,
        EvidenceStatus::Failed(_) => WorkbenchReviewEvidenceState::Failed,
        EvidenceStatus::Stale(_) => WorkbenchReviewEvidenceState::Stale,
    };
    let provenance = value.record().map(peritus_run_settlement::EvidenceRecord::provenance);
    WorkbenchReviewEvidence::new(
        kind,
        state,
        provenance.map(peritus_run_settlement::CandidateIdentity::candidate_digest),
        provenance.map(peritus_run_settlement::CandidateIdentity::conversation_revision),
        provenance.map(peritus_run_settlement::CandidateIdentity::checkpoint_sequence),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}
