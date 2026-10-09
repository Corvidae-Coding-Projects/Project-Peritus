//! Preview binding checks and graphical-goal evidence qualification.

use super::*;
mod goal;

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
        let receipt = self.admit_preview_behavior(run, command, launch, observed)?;
        self.qualify_graphical_goal(run, command, launch)?;
        Ok(receipt)
    }

    /// Persists the observed fact before its separately retryable goal publication.
    pub(in crate::product_run) fn admit_preview_behavior(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch: ControlOperationId,
        observed: &WorkbenchLaunchText,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        // The durable receipt is authoritative on retry, even after output cleanup or restart.
        let prior = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            record.preview.operations.get(&command.operation()).cloned()
        };
        if let Some(prior) = prior {
            let fingerprint = command.fingerprint()?;
            if prior.fingerprint != fingerprint {
                return Err(app_error(Code::IdempotencyConflict));
            }
            return receipt(command, prior.accepted_revision, fingerprint);
        }
        self.refresh_previews(run)?;
        let current = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            require_launch(&record.preview, launch)?.clone()
        };
        if current.state() == WorkbenchLaunchState::Accepted {
            return Err(app_error(Code::StaleRevision));
        }
        let process_id = current.process().ok_or_else(|| app_error(Code::StaleRevision))?;
        let observed_match = self
            .preview_output_match(command.query(), launch, &current, process_id, observed.as_str())?
            .ok_or_else(|| app_error(Code::StaleRevision))?;
        let WorkbenchIntent::CheckPreviewBehavior { note, .. } = command.intent() else {
            return Err(app_error(Code::MalformedFrame));
        };
        let note_match = self.preview_output_match(
            command.query(),
            launch,
            &current,
            process_id,
            note.as_str(),
        )?;
        let goal = if note_match.is_some() {
            self.preview_goal_binding(run, note.as_str())?
        } else {
            None
        };
        let evidence = super::super::super::preview_evidence::PreviewBehaviorEvidence::new(
            command,
            observed_match,
            note_match,
            goal,
        )?;
        evidence.validate(command, &current)?;
        let (receipt, _) =
            self.admit_preview_with_evidence(command, run, Some(evidence), |preview| {
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

    fn preview_output_match(
        &self,
        query: WorkbenchQuery,
        launch_id: ControlOperationId,
        launch: &WorkbenchLaunchResult,
        process_id: peritus_types::ProcessId,
        needle: &str,
    ) -> Result<Option<peritus_product_runner::PreviewOutputMatch>, AppProtocolError> {
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
                .preview_output_match(process_id, needle)
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
        runtime.preview_output_match(process_id, needle).map_err(|_| app_error(Code::Backpressure))
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
