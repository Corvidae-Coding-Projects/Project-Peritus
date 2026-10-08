//! Preview binding checks and graphical-goal evidence qualification.

use super::*;

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;

impl ProductRunService {
    pub(super) fn check_preview_behavior(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch: ControlOperationId,
        observed: &WorkbenchLaunchText,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        self.refresh_previews(run)?;
        let (current, process_id) = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            let current = require_launch(&record.preview, launch)?;
            (current.clone(), current.process())
        };
        if current.state() == WorkbenchLaunchState::Accepted {
            return Err(app_error(Code::StaleRevision));
        }
        let process_id = process_id.ok_or_else(|| app_error(Code::StaleRevision))?;
        if !self.preview_output_contains(
            command.query(),
            launch,
            &current,
            process_id,
            observed.as_str(),
        )? {
            return Err(app_error(Code::StaleRevision));
        }
        let (receipt, _) = self.admit_preview(command, run, |preview| {
            mutate_launch(preview, launch, |current| {
                rebuild_launch_with(
                    current,
                    current.interactions().to_vec(),
                    current.captures().to_vec(),
                    current.feedback().to_vec(),
                    current.behavior_checks().saturating_add(1),
                )
            })
        })?;
        self.qualify_graphical_goal(run, command, launch)?;
        Ok(receipt)
    }

    pub(super) fn add_artifact_feedback(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        capture: ControlOperationId,
        feedback: peritus_app_protocol::WorkbenchReviewFeedback,
        message: peritus_app_protocol::WorkbenchInputText,
        region: Option<WorkbenchArtifactRegion>,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        self.admit_preview(command, run, |preview| {
            let (launch, dimensions) = find_capture(preview, capture)?;
            if region.is_some_and(|value| !region_within(value, dimensions)) {
                return Err(app_error(Code::MalformedFrame));
            }
            mutate_launch(preview, launch, |current| {
                let mut rows = current.feedback().to_vec();
                rows.push(WorkbenchArtifactFeedback::new(
                    command.operation(),
                    capture,
                    feedback,
                    message.clone(),
                    region,
                ));
                rebuild_launch_with(
                    current,
                    current.interactions().to_vec(),
                    current.captures().to_vec(),
                    rows,
                    current.behavior_checks(),
                )
            })
        })
        .map(|(receipt, _)| receipt)
    }

    pub(super) fn preview_run(
        &self,
        command: &WorkbenchCommand,
    ) -> Result<RunId, AppProtocolError> {
        if let WorkbenchIntent::StartPreview(profile) = command.intent() {
            return Ok(profile.run());
        }
        let target = match command.intent() {
            WorkbenchIntent::InteractPreview { launch, .. }
            | WorkbenchIntent::StopPreview { launch }
            | WorkbenchIntent::CheckPreviewBehavior { launch, .. } => {
                PreviewTarget::Launch(*launch)
            }
            WorkbenchIntent::CapturePreview(request) => PreviewTarget::Launch(request.launch()),
            WorkbenchIntent::AddArtifactFeedback { capture, .. } => {
                PreviewTarget::Capture(*capture)
            }
            _ => return Err(app_error(Code::MalformedFrame)),
        };
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        let mut matches = records.iter().filter(|(_, record)| {
            record_matches_query(record, command.query())
                && match target {
                    PreviewTarget::Launch(value) => has_launch(&record.preview, value),
                    PreviewTarget::Capture(value) => find_capture(&record.preview, value).is_ok(),
                }
        });
        let run = matches
            .next()
            .map(|(run, _)| *run)
            .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        if matches.next().is_some() {
            return Err(app_error(Code::Internal));
        }
        Ok(run)
    }

    pub(super) fn validate_preview_binding(
        &self,
        run: RunId,
        query: WorkbenchQuery,
    ) -> Result<(), AppProtocolError> {
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        if !record_matches_query(record, query) {
            return Err(app_error(Code::SessionMismatch));
        }
        Ok(())
    }

    pub(super) fn verify_profile(
        &self,
        query: WorkbenchQuery,
        profile: &WorkbenchLaunchProfile,
    ) -> Result<(PathBuf, bool), AppProtocolError> {
        let workspace = self
            .inner
            .workspaces
            .get(&query.workspace())
            .cloned()
            .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let direct = self.inner.folders.contains_key(&query.workspace());
        match (direct, profile.source().kind()) {
            (true, WorkbenchLaunchSourceKind::PlainFolderFile) => {
                let source =
                    resolve_workspace_path(&workspace, profile.source().path().as_str(), false)?;
                if file_digest(&source)? != profile.source().digest() {
                    return Err(app_error(Code::StaleRevision));
                }
            }
            (false, WorkbenchLaunchSourceKind::ManagedCandidate) => {
                if ProductRunner::candidate_digest(&workspace)
                    .map_err(|_| app_error(Code::Backpressure))?
                    != profile.source().digest()
                {
                    return Err(app_error(Code::StaleRevision));
                }
            }
            _ => return Err(app_error(Code::MalformedFrame)),
        }
        if let Some(build) = profile.build() {
            let path = resolve_workspace_path(&workspace, build.path().as_str(), false)?;
            if file_digest(&path)? != build.digest() {
                return Err(app_error(Code::StaleRevision));
            }
        }
        let _ = resolve_workspace_path(&workspace, profile.working_directory().as_str(), true)?;
        Ok((workspace, direct))
    }

    pub(super) fn qualify_graphical_goal(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch_id: ControlOperationId,
    ) -> Result<(), AppProtocolError> {
        let WorkbenchIntent::CheckPreviewBehavior { note, .. } = command.intent() else {
            return Ok(());
        };
        let query = command.query();
        let (start, launch, operations) = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            let start = record.interaction.workbench.clone();
            let launch = require_launch(&record.preview, launch_id)?.clone();
            (start, launch, record.preview.operations.clone())
        };
        if !matches!(start.intent(), ControlIntent::StartGoal { .. })
            || launch.profile().run() != run
            || launch.profile().build().is_none()
            || launch.process().is_none()
            || !launch.ready()
            || !matches!(
                launch.state(),
                WorkbenchLaunchState::Running
                    | WorkbenchLaunchState::Exited
                    | WorkbenchLaunchState::Stopped
            )
            || launch.behavior_checks() == 0
        {
            return Ok(());
        }
        let capture = launch.captures().iter().find(|capture| {
            capture.state() == WorkbenchCaptureState::Captured
                && capture.artifact().is_some()
                && capture.image_digest().is_some()
                && capture.dimensions().is_some_and(|(width, height)| width > 0 && height > 0)
                && capture.captured_unix_millis().is_some()
                && ordered_capture(
                    capture.operation(),
                    command.operation(),
                    launch.interactions(),
                    &operations,
                )
        });
        let Some(capture) = capture else { return Ok(()) };
        let process_id = launch.process().ok_or_else(|| app_error(Code::StaleRevision))?;
        if !self.preview_output_contains(query, launch_id, &launch, process_id, note.as_str())? {
            return Ok(());
        }
        match self.verify_profile(query, launch.profile()) {
            Ok(_) => {}
            Err(error) if error.code() == Code::StaleRevision => return Ok(()),
            Err(error) => return Err(error),
        }
        self.with_controls(false, |store| {
            let record = store.load(start.conversation())?.ok_or(ControlError::NotFound)?;
            let goal = record
                .goal()
                .filter(|goal| goal.id() == start.id())
                .ok_or(ControlError::NotFound)?;
            if goal.run_bytes() != run.as_bytes() {
                return Ok(());
            }
            let mut matching = goal.criteria().iter().enumerate().filter(|(_, criterion)| {
                criterion.kind() == GoalCriterionKind::GraphicalPlaytest
                    && criterion.description() == note.as_str()
            });
            let Some((index, _)) = matching.next() else { return Ok(()) };
            if matching.next().is_some() {
                return Err(ControlError::InvalidInput.into());
            }
            let index = u32::try_from(index).map_err(|_| ControlError::Capacity)?;
            store.observe_graphical_goal_evidence(
                &start,
                index,
                goal.user_revision(),
                goal.required_input_generation(),
                OperationId::new(launch_id.into_bytes())?,
                OperationId::new(capture.operation().into_bytes())?,
            )
        })
        .map_err(error_value)
    }

    fn preview_output_contains(
        &self,
        query: WorkbenchQuery,
        launch_id: ControlOperationId,
        launch: &WorkbenchLaunchResult,
        process_id: peritus_types::ProcessId,
        needle: &str,
    ) -> Result<bool, AppProtocolError> {
        let (workspace, direct) = self.verify_profile(query, launch.profile())?;
        let active = self
            .inner
            .preview_processes
            .lock()
            .map_err(|_| app_error(Code::Backpressure))?
            .get(&launch_id)
            .cloned();
        if let Some(active) = active
            && active.launch.process_id() == process_id
        {
            return active
                .runtime
                .preview_output_contains(process_id, needle)
                .map_err(|_| app_error(Code::Backpressure));
        }
        let run = RunId::new(launch_id.into_bytes()).map_err(|_| app_error(Code::Internal))?;
        let state = self.preview_state_root().join("commands").join(hex(launch_id.as_bytes()));
        let runtime = if direct {
            CommandRuntime::open_direct(state, workspace, run, self.inner.processes.clone())
        } else {
            CommandRuntime::open(state, workspace, run, self.inner.processes.clone())
        }
        .map_err(|_| app_error(Code::Backpressure))?;
        runtime
            .preview_output_contains(process_id, needle)
            .map_err(|_| app_error(Code::Backpressure))
    }

    pub(super) fn preview_state_root(&self) -> PathBuf {
        self.inner
            .directory
            .parent()
            .expect("product-run directory always has state parent")
            .join("workbench-v1")
            .join("previews")
    }
}

fn ordered_capture(
    capture: ControlOperationId,
    check: ControlOperationId,
    interactions: &[WorkbenchInteractionReceipt],
    operations: &std::collections::BTreeMap<
        ControlOperationId,
        super::super::super::PreviewOperationRecord,
    >,
) -> bool {
    let Some(capture) = operations.get(&capture) else { return false };
    capture.result_sequence > 0
        && operations
            .get(&check)
            .is_some_and(|check| capture.result_sequence < check.result_sequence)
        && interactions.iter().any(|interaction| {
            interaction.observed()
                && operations.get(&interaction.operation()).is_some_and(|input| {
                    input.completed_sequence > 0
                        && input.completed_sequence < capture.result_sequence
                })
        })
}

pub(super) fn record_matches_query(
    record: &super::super::super::RunRecord,
    query: WorkbenchQuery,
) -> bool {
    let operation = &record.interaction.workbench;
    operation.conversation().as_bytes() == query.conversation().as_bytes()
        && operation.workspace_bytes() == query.workspace().as_bytes()
}

pub(super) fn active_launch_run(
    service: &ProductRunService,
    run: RunId,
    launch: ControlOperationId,
) -> Result<peritus_types::WorkspaceId, AppProtocolError> {
    let records = service.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
    let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
    if !has_launch(&record.preview, launch) {
        return Err(app_error(Code::InvalidIdentifier));
    }
    Ok(record.request.workspace_id())
}

pub(super) fn preview_launches(
    preview: &super::super::super::PreviewAggregate,
) -> Vec<WorkbenchLaunchResult> {
    preview.page.as_ref().map_or_else(Vec::new, |page| page.launches().to_vec())
}

pub(super) fn resolve_workspace_path(
    workspace: &Path,
    relative: &str,
    directory: bool,
) -> Result<PathBuf, AppProtocolError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err(app_error(Code::MalformedFrame));
    }
    let workspace = workspace.canonicalize().map_err(|_| app_error(Code::InvalidIdentifier))?;
    let resolved =
        workspace.join(relative).canonicalize().map_err(|_| app_error(Code::InvalidIdentifier))?;
    if !resolved.starts_with(&workspace)
        || if directory { !resolved.is_dir() } else { !resolved.is_file() }
    {
        return Err(app_error(Code::InvalidIdentifier));
    }
    Ok(resolved)
}

pub(super) fn file_digest(path: &Path) -> Result<Sha256Digest, AppProtocolError> {
    use sha2::Digest as _;
    let mut file = fs::File::open(path).map_err(|_| app_error(Code::InvalidIdentifier))?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1_024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer).map_err(|_| app_error(Code::Backpressure))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(Sha256Digest::new(digest.finalize().into()))
}

pub(super) fn capture_capability() -> WorkbenchCaptureCapability {
    let host = super::super::super::PreviewCaptureHost::discover();
    capture_capability_for(&host)
}
