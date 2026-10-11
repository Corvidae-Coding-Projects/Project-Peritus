//! Exact review-page and summary projections from retained run state.

use std::path::PathBuf;

use super::{
    ActorId, ControlError, ConversationId, ConversationRecord, Error, ProductRunService,
    ProductRunner, ReviewAnchor, ReviewFeedback, ReviewRange, ReviewTarget, RunId, Sha256Digest,
    WorkbenchReviewAnchor, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewFeedback, WorkbenchReviewPage, WorkbenchReviewQuery, WorkbenchReviewTarget,
    missing, parse_workbench_diff, parse_workbench_diff_page,
    parse_workbench_diff_page_with_anchors, project_anchor, project_comment,
    project_comment_with_current, project_evidence, sha256,
};

pub(super) fn current_page(
    service: &ProductRunService,
    actor: ActorId,
    requested: WorkbenchReviewQuery,
) -> Result<WorkbenchReviewPage, Error> {
    let record = load_control(service, actor, requested.query())?;
    if requested.revision() != 0 && requested.revision() != record.revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let current = current_snapshot(service, &record, requested.run())?;
    let total =
        u32::try_from(record.reviews().comments().len()).map_err(|_| ControlError::Capacity)?;
    let (diff, files) = parse_workbench_diff(
        requested.run(),
        requested.query().workspace(),
        current.candidate,
        &current.raw_diff,
    )
    .map_err(|_| ControlError::InvalidInput)?;
    let comments = record
        .reviews()
        .comments()
        .iter()
        .skip(requested.offset() as usize)
        .take(peritus_app_protocol::MAX_WORKBENCH_REVIEW_PAGE)
        .map(|comment| project_comment(comment, &files))
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = current_evidence(service, requested.run())?;
    let query = WorkbenchReviewQuery::new(
        requested.query(),
        requested.run(),
        record.revision(),
        requested.offset(),
    );
    WorkbenchReviewPage::new(query, current.candidate, diff, files, comments, total, evidence)
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn current_summary(
    service: &ProductRunService,
    actor: ActorId,
    requested: WorkbenchReviewQuery,
) -> Result<peritus_app_protocol::WorkbenchReviewSummary, Error> {
    let record = load_control(service, actor, requested.query())?;
    if requested.revision() != 0 && requested.revision() != record.revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let current = current_snapshot(service, &record, requested.run())?;
    let total =
        u32::try_from(record.reviews().comments().len()).map_err(|_| ControlError::Capacity)?;
    let selected = record
        .reviews()
        .comments()
        .iter()
        .skip(requested.offset() as usize)
        .take(peritus_app_protocol::MAX_WORKBENCH_REVIEW_PAGE)
        .collect::<Vec<_>>();
    let anchors = selected
        .iter()
        .map(|comment| project_anchor(comment.anchor()))
        .collect::<Result<Vec<_>, _>>()?;
    let cursor = peritus_app_protocol::WorkbenchReviewDiffQuery::new(
        requested.query(),
        requested.run(),
        record.revision(),
        0,
        0,
        0,
    );
    let parsed = parse_workbench_diff_page_with_anchors(
        cursor,
        current.candidate,
        &current.raw_diff,
        &anchors,
    );
    let (files, hunks, lines, available, matches) = match parsed {
        Ok((page, matches)) => {
            (page.total_files(), page.total_hunks(), page.total_lines(), true, matches)
        }
        Err(_) => (0, 0, 0, false, vec![false; anchors.len()]),
    };
    let comments = selected
        .into_iter()
        .zip(matches)
        .map(|(comment, is_current)| project_comment_with_current(comment, is_current))
        .collect::<Result<Vec<_>, _>>()?;
    let query = WorkbenchReviewQuery::new(
        requested.query(),
        requested.run(),
        record.revision(),
        requested.offset(),
    );
    peritus_app_protocol::WorkbenchReviewSummary::new(
        query,
        current.candidate,
        sha256(current.raw_diff.as_bytes()),
        files,
        hunks,
        lines,
        available,
        comments,
        total,
        current_evidence(service, requested.run())?,
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

fn current_evidence(
    service: &ProductRunService,
    run: RunId,
) -> Result<Vec<WorkbenchReviewEvidence>, Error> {
    let records =
        service.inner.records.read().map_err(|_| Error::Corrupt("run owner lock poisoned"))?;
    let record = records.get(&run).ok_or(ControlError::NotFound)?;
    record
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
        .collect::<Result<Vec<_>, _>>()
}

pub(super) fn load_control(
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

pub(super) struct CurrentSnapshot {
    pub(super) candidate: Sha256Digest,
    pub(super) raw_diff: String,
}

pub(super) fn current_snapshot(
    service: &ProductRunService,
    control: &ConversationRecord,
    run: RunId,
) -> Result<CurrentSnapshot, Error> {
    let records =
        service.inner.records.read().map_err(|_| Error::Corrupt("run owner lock poisoned"))?;
    let record = records.get(&run).ok_or(ControlError::NotFound)?;
    let start = &record.interaction.workbench;
    if start.conversation() != control.id()
        || start.workspace_bytes() != control.workspace_bytes()
        || start.actor_bytes() != control.owner_bytes()
        || record.request.workspace_id().as_bytes() != control.workspace_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    if !super::super::super::operation::may_start_execution(service, record)
        .map_err(|_| Error::Corrupt("operation projection unavailable"))?
    {
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
        .map_or(live_candidate, |checkpoint| checkpoint.identity().repository_digest());
    if candidate != live_candidate {
        return Err(ControlError::StaleRevision.into());
    }
    Ok(CurrentSnapshot { candidate, raw_diff: record.snapshot.diff().to_owned() })
}

pub(super) fn current_structured_diff_page(
    service: &ProductRunService,
    actor: ActorId,
    requested: peritus_app_protocol::WorkbenchReviewDiffQuery,
) -> Result<peritus_app_protocol::WorkbenchReviewDiffPage, Error> {
    let control = load_control(service, actor, requested.query())?;
    if requested.revision() != 0 && requested.revision() != control.revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let current = current_snapshot(service, &control, requested.run())?;
    let query = peritus_app_protocol::WorkbenchReviewDiffQuery::new(
        requested.query(),
        requested.run(),
        control.revision(),
        requested.file_offset(),
        requested.hunk_offset(),
        requested.line_offset(),
    );
    parse_workbench_diff_page(query, current.candidate, &current.raw_diff)
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn current_raw_diff(
    service: &ProductRunService,
    actor: ActorId,
    requested: peritus_app_protocol::WorkbenchReviewDiffBytesQuery,
) -> Result<peritus_app_protocol::WorkbenchReviewDiffBytes, Error> {
    let record = load_control(service, actor, requested.query())?;
    if requested.revision() != record.revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let current = current_snapshot(service, &record, requested.run())?;
    if current.candidate != requested.candidate_digest()
        || sha256(current.raw_diff.as_bytes()) != requested.diff_digest()
    {
        return Err(ControlError::StaleRevision.into());
    }
    let total = u32::try_from(current.raw_diff.len()).map_err(|_| ControlError::Capacity)?;
    let start = usize::try_from(requested.offset()).map_err(|_| ControlError::InvalidInput)?;
    let length = usize::try_from(requested.maximum_bytes())
        .map_err(|_| ControlError::InvalidInput)?
        .min(peritus_app_protocol::MAX_WORKBENCH_REVIEW_DIFF_BYTES);
    if start > current.raw_diff.len() {
        return Err(ControlError::InvalidInput.into());
    }
    let end = start.saturating_add(length).min(current.raw_diff.len());
    peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        requested,
        total,
        current.raw_diff.as_bytes()[start..end].to_vec(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
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
