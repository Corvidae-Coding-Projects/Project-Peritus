//! Structured review projection and exact-anchor mutation admission.

use std::path::PathBuf;

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
    parse_workbench_diff,
};
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
    let current = current_targets(service, &record, run)?;
    if let Some(anchor) = supplied
        && (anchor.workspace() != command.query().workspace()
            || anchor.run() != run
            || !contains(&current.files, anchor))
    {
        return Err(ControlError::StaleRevision.into());
    }
    if let Some(prior) = prior {
        let prior = project_anchor(prior)?;
        if contains(&current.files, &prior) {
            // Rebinding is an explicit recovery operation, never an ordinary anchor edit.
            return Err(ControlError::InvalidInput.into());
        }
    }
    Ok(())
}

/// Starts only the work explicitly denoted by a newly accepted review input. Public legacy
/// continuation remains unavailable for governed runs, and preferences/constraints start nothing.
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
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
        if !record.snapshot.phase().terminal() {
            return Err(ProductRunServiceError::InvalidState);
        }
        let mut options = record.interaction.clone().ok_or(ProductRunServiceError::InvalidState)?;
        let providers =
            service.resolve_selected_providers(record.request.providers(), Some(&options))?;
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
        record.interaction = Some(options);
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
            Arc::clone(&record.conversation),
            record.finding_state.clone(),
            record.resume.clone(),
        )
    };
    service
        .spawn(launch.0, launch.1, launch.2, launch.3, launch.4, launch.5, launch.6, launch.7)
        .await;
    Ok(())
}

fn current_page(
    service: &ProductRunService,
    actor: ActorId,
    requested: WorkbenchReviewQuery,
) -> Result<WorkbenchReviewPage, Error> {
    let record = load_control(service, actor, requested.query())?;
    if requested.revision() != 0 && requested.revision() != record.revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let current = current_targets(service, &record, requested.run())?;
    let total =
        u32::try_from(record.reviews().comments().len()).map_err(|_| ControlError::Capacity)?;
    let comments = record
        .reviews()
        .comments()
        .iter()
        .skip(requested.offset() as usize)
        .take(peritus_app_protocol::MAX_WORKBENCH_REVIEW_PAGE)
        .map(|comment| project_comment(comment, &current.files))
        .collect::<Result<Vec<_>, _>>()?;
    let query = WorkbenchReviewQuery::new(
        requested.query(),
        requested.run(),
        record.revision(),
        requested.offset(),
    );
    WorkbenchReviewPage::new(
        query,
        current.candidate,
        current.diff,
        current.files,
        comments,
        total,
        current.evidence,
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

fn load_control(
    service: &ProductRunService,
    actor: ActorId,
    query: peritus_app_protocol::WorkbenchQuery,
) -> Result<ConversationRecord, Error> {
    service.control_workspace(query)?;
    let id = ConversationId::new(query.conversation().into_bytes())?;
    let record =
        service.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != query.workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    Ok(record)
}

struct CurrentReview {
    candidate: Sha256Digest,
    diff: Sha256Digest,
    files: Vec<WorkbenchDiffFile>,
    evidence: Vec<WorkbenchReviewEvidence>,
}

fn current_targets(
    service: &ProductRunService,
    control: &ConversationRecord,
    run: RunId,
) -> Result<CurrentReview, Error> {
    let records =
        service.inner.records.read().map_err(|_| Error::Corrupt("run owner lock poisoned"))?;
    let record = records.get(&run).ok_or(ControlError::NotFound)?;
    let start = record
        .interaction
        .as_ref()
        .and_then(|options| options.workbench.as_ref())
        .ok_or(ControlError::ScopeMismatch)?;
    if start.conversation() != control.id()
        || start.workspace_bytes() != control.workspace_bytes()
        || start.actor_bytes() != control.owner_bytes()
        || record.request.workspace_id().as_bytes() != control.workspace_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    if !record.snapshot.phase().terminal() {
        return Err(ControlError::InvalidInput.into());
    }
    let workspace = service
        .inner
        .workspaces
        .get(&record.request.workspace_id())
        .ok_or(ControlError::ScopeMismatch)?;
    let live_candidate = ProductRunner::candidate_digest(workspace)
        .map_err(|_| Error::Corrupt("candidate digest unavailable"))?;
    let candidate = record
        .checkpoint
        .as_ref()
        .map_or(live_candidate, |checkpoint| checkpoint.identity().candidate_digest());
    if candidate != live_candidate {
        return Err(ControlError::StaleRevision.into());
    }
    let (diff, files) =
        parse_workbench_diff(run, record.request.workspace_id(), candidate, record.snapshot.diff())
            .map_err(|_| ControlError::InvalidInput)?;
    let evidence = record
        .checkpoint
        .as_ref()
        .map_or_else(
            || {
                vec![
                    missing(WorkbenchReviewEvidenceKind::Checks),
                    missing(WorkbenchReviewEvidenceKind::IndependentReview),
                ]
            },
            |checkpoint| {
                vec![
                    project_evidence(WorkbenchReviewEvidenceKind::Checks, checkpoint.gates()),
                    project_evidence(
                        WorkbenchReviewEvidenceKind::IndependentReview,
                        checkpoint.review(),
                    ),
                ]
            },
        )
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CurrentReview { candidate, diff, files, evidence })
}

fn contains(files: &[WorkbenchDiffFile], anchor: &WorkbenchReviewAnchor) -> bool {
    files.iter().any(|file| {
        file.anchor() == anchor || file.hunks().iter().any(|hunk| hunk.anchor() == anchor)
    })
}

pub(super) fn domain_anchor(value: &WorkbenchReviewAnchor) -> Result<ReviewAnchor, ControlError> {
    let target = match value.target() {
        WorkbenchReviewTarget::File => ReviewTarget::File,
        WorkbenchReviewTarget::Hunk => ReviewTarget::Hunk,
    };
    let range = match target {
        ReviewTarget::File => ReviewRange::file(),
        ReviewTarget::Hunk => ReviewRange::hunk(
            value.range().old_start(),
            value.range().old_lines(),
            value.range().new_start(),
            value.range().new_lines(),
        )?,
    };
    ReviewAnchor::new(
        value.run(),
        value.workspace(),
        value.candidate_digest(),
        value.diff_digest(),
        PathBuf::from(value.path()),
        value.before_blob_digest(),
        value.after_blob_digest(),
        value.context_digest(),
        target,
        range,
    )
}

pub(super) const fn domain_feedback(value: WorkbenchReviewFeedback) -> ReviewFeedback {
    match value {
        WorkbenchReviewFeedback::Explain => ReviewFeedback::Explain,
        WorkbenchReviewFeedback::RequestRevision => ReviewFeedback::RequestRevision,
        WorkbenchReviewFeedback::KeepBehavior => ReviewFeedback::KeepBehavior,
        WorkbenchReviewFeedback::LeaveAlone => ReviewFeedback::LeaveAlone,
    }
}

mod projection;
use projection::{missing, project_anchor, project_comment, project_evidence};
