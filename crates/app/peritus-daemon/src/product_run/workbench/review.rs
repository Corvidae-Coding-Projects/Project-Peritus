//! Structured review projection and exact-anchor mutation admission.

use super::{Error, ProductRunService, error_response};
use crate::product_run::{
    ProductRunServiceError, RunProgress, initial_snapshot, persist_record, replace_snapshot,
    workspace_has_active_run,
};
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WorkbenchCommand, WorkbenchDiffFile, WorkbenchInputId,
    WorkbenchInputSelection, WorkbenchInputText, WorkbenchIntent, WorkbenchReviewAnchor,
    WorkbenchReviewComment, WorkbenchReviewCommentState, WorkbenchReviewEvidence,
    WorkbenchReviewEvidenceKind, WorkbenchReviewEvidenceState, WorkbenchReviewFeedback,
    WorkbenchReviewPage, WorkbenchReviewQuery, WorkbenchReviewRange, WorkbenchReviewTarget,
    parse_workbench_diff, parse_workbench_diff_page, parse_workbench_diff_page_with_anchors,
};
use peritus_codec::sha256;
use peritus_product_runner::{
    ProductRunner,
    control::{
        ControlError, ConversationId, ConversationRecord, OperationId, ReviewAnchor,
        ReviewCommentState, ReviewFeedback, ReviewRange, ReviewTarget,
    },
};
use peritus_provider_core::CancellationToken;
use peritus_run_settlement::{EvidenceStatus, QualificationEvidence};
use peritus_types::{ActorId, RunId, Sha256Digest};
use std::sync::{Arc, atomic::AtomicBool};

impl ProductRunService {
    pub(crate) fn workbench_review(
        &self,
        actor: ActorId,
        query: WorkbenchReviewQuery,
    ) -> AppResponsePayload {
        current_page(self, actor, query)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReview)
    }

    pub(crate) fn workbench_review_summary(
        &self,
        actor: ActorId,
        query: WorkbenchReviewQuery,
    ) -> AppResponsePayload {
        current_summary(self, actor, query)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReviewSummary)
    }

    pub(crate) fn workbench_review_diff(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::WorkbenchReviewDiffQuery,
    ) -> AppResponsePayload {
        current_structured_diff_page(self, actor, query)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReviewDiff)
    }

    pub(crate) fn workbench_review_diff_bytes(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::WorkbenchReviewDiffBytesQuery,
    ) -> AppResponsePayload {
        current_raw_diff(self, actor, query)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReviewDiffBytes)
    }
}

pub(super) fn validate_command(
    service: &ProductRunService,
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<(), Error> {
    let record = load_control(service, actor, command.query())?;
    let (run, supplied, prior) = match command.intent() {
        WorkbenchIntent::AddReview { anchor, .. } => (anchor.run(), Some(anchor), None),
        WorkbenchIntent::RebindReview { comment, anchor } => {
            let id = OperationId::new(comment.into_bytes())?;
            let prior = record.reviews().comment(id).ok_or(ControlError::NotFound)?;
            (anchor.run(), Some(anchor), Some(prior.anchor()))
        }
        WorkbenchIntent::DismissReview { comment } => {
            let id = OperationId::new(comment.into_bytes())?;
            let prior = record.reviews().comment(id).ok_or(ControlError::NotFound)?;
            (prior.anchor().run(), None, None)
        }
        _ => return Ok(()),
    };
    let current = current_snapshot(service, &record, run)?;
    let mut anchors = Vec::new();
    if let Some(anchor) = supplied {
        anchors.push(anchor.clone());
    }
    if let Some(prior) = prior {
        anchors.push(project_anchor(prior)?);
    }
    let matches = if anchors.is_empty()
        || !current.raw_diff.lines().any(|line| line.starts_with("diff --git "))
    {
        vec![false; anchors.len()]
    } else {
        let cursor = peritus_app_protocol::WorkbenchReviewDiffQuery::new(
            command.query(),
            run,
            record.revision(),
            0,
            0,
            0,
        );
        parse_workbench_diff_page_with_anchors(
            cursor,
            current.candidate,
            &current.raw_diff,
            &anchors,
        )
        .map_err(|_| ControlError::InvalidInput)?
        .1
    };
    let mut match_index = 0;
    if let Some(anchor) = supplied
        && (anchor.workspace() != command.query().workspace()
            || anchor.run() != run
            || !matches.get(match_index).copied().unwrap_or(false))
    {
        return Err(ControlError::StaleRevision.into());
    }
    if supplied.is_some() {
        match_index += 1;
    }
    if prior.is_some() && matches.get(match_index).copied().unwrap_or(false) {
        // Rebinding is an explicit recovery operation, never an ordinary anchor edit.
        return Err(ControlError::InvalidInput.into());
    }
    Ok(())
}

/// Starts only the work explicitly denoted by a newly accepted review input.
/// Preferences and constraints do not start execution.
pub(super) async fn resume_feedback(
    service: &ProductRunService,
    actor: ActorId,
    command: &WorkbenchCommand,
) -> Result<(), ProductRunServiceError> {
    let (run, feedback, activity) = match command.intent() {
        WorkbenchIntent::AddReview { anchor, feedback, message } => {
            (anchor.run(), *feedback, message.as_str().to_owned())
        }
        WorkbenchIntent::RebindReview { comment, .. } => {
            let record = load_control(service, actor, command.query())?;
            let comment = record
                .reviews()
                .comment(
                    OperationId::new(comment.into_bytes())
                        .map_err(ProductRunServiceError::Control)?,
                )
                .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
            (
                comment.anchor().run(),
                match comment.feedback() {
                    ReviewFeedback::Explain => WorkbenchReviewFeedback::Explain,
                    ReviewFeedback::RequestRevision => WorkbenchReviewFeedback::RequestRevision,
                    ReviewFeedback::KeepBehavior => WorkbenchReviewFeedback::KeepBehavior,
                    ReviewFeedback::LeaveAlone => WorkbenchReviewFeedback::LeaveAlone,
                },
                "Rebound stale review feedback to the newly selected exact target.".to_owned(),
            )
        }
        WorkbenchIntent::DismissReview { .. } => return Ok(()),
        _ => return Ok(()),
    };
    let mode = match feedback {
        WorkbenchReviewFeedback::Explain => peritus_app_protocol::ProductInteractionMode::Review,
        WorkbenchReviewFeedback::RequestRevision => {
            peritus_app_protocol::ProductInteractionMode::Build
        }
        WorkbenchReviewFeedback::KeepBehavior | WorkbenchReviewFeedback::LeaveAlone => {
            return Ok(());
        }
    };
    let launch = {
        let mut records =
            service.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let workspace =
            records.get(&run).ok_or(ProductRunServiceError::NotFound)?.request.workspace_id();
        if workspace_has_active_run(&records, workspace, Some(run)) {
            return Err(ProductRunServiceError::InvalidState);
        }
        crate::product_run::deliverable::discard::workspace_available(
            &service.inner.directory,
            &records,
            workspace,
        )?;
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
        if !super::super::operation::may_start_execution(service, record)? {
            return Err(ProductRunServiceError::InvalidState);
        }
        let mut options = record.interaction.clone();
        let providers = service.resolve_selected_providers(record.request.providers(), &options)?;
        let root = service
            .inner
            .workspaces
            .get(&workspace)
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        options.mode = mode;
        options.append(
            peritus_app_protocol::ProductActivityKind::User,
            &activity,
            match feedback {
                WorkbenchReviewFeedback::Explain => {
                    "Anchored explanation admitted with read-only review authority"
                }
                WorkbenchReviewFeedback::RequestRevision => {
                    "Anchored revision admitted through the ordinary qualified pipeline"
                }
                _ => panic!("non-running feedback returned above"),
            },
        )?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = CancellationToken::new();
        record.cancelled = Arc::clone(&cancelled);
        record.provider_cancellation = token.clone();
        record.interaction = options;
        record.snapshot = initial_snapshot(&record.request)?;
        record.snapshot = replace_snapshot(
            &record.snapshot,
            peritus_app_protocol::ProductRunPhase::Queued,
            match feedback {
                WorkbenchReviewFeedback::Explain => {
                    "Anchored explanation queued for read-only review"
                }
                WorkbenchReviewFeedback::RequestRevision => {
                    "Anchored revision queued for the writer"
                }
                _ => panic!("non-running feedback returned above"),
            },
            "",
        )?;
        record.progress = RunProgress::default();
        record.settlement = None;
        record.interruption_cause.clear();
        persist_record(&service.inner.directory, record)?;
        (
            record.request.clone(),
            root,
            providers,
            cancelled,
            token,
            record.finding_state.clone(),
            record.resume.clone(),
        )
    };
    service.spawn(launch.0, launch.1, launch.2, launch.3, launch.4, launch.5, launch.6).await;
    Ok(())
}

mod projection;
use projection::{
    missing, project_anchor, project_comment, project_comment_with_current, project_evidence,
};
mod current;
use current::{
    current_page, current_raw_diff, current_snapshot, current_structured_diff_page,
    current_summary, load_control,
};
pub(super) fn domain_anchor(value: &WorkbenchReviewAnchor) -> Result<ReviewAnchor, ControlError> {
    current::domain_anchor(value)
}
pub(super) const fn domain_feedback(value: WorkbenchReviewFeedback) -> ReviewFeedback {
    current::domain_feedback(value)
}

pub(super) fn contains(files: &[WorkbenchDiffFile], anchor: &WorkbenchReviewAnchor) -> bool {
    files.iter().any(|file| {
        file.anchor() == anchor || file.hunks().iter().any(|hunk| hunk.anchor() == anchor)
    })
}
