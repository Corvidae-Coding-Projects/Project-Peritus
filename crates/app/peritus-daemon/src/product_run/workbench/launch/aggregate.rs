//! Durable preview aggregate admission and immutable result rebuilding.

use super::*;

impl ProductRunService {
    pub(super) fn admit_preview(
        &self,
        command: &WorkbenchCommand,
        run: RunId,
        mutate: impl FnOnce(&mut super::super::super::PreviewAggregate) -> Result<(), AppProtocolError>,
    ) -> Result<(WorkbenchReceipt, bool), AppProtocolError> {
        let fingerprint = command.fingerprint().map_err(|_| app_error(Code::MalformedFrame))?;
        let mut records = self.inner.records.write().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get_mut(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        if let Some(prior) = record.preview.operations.get(&command.operation()) {
            if prior.fingerprint != fingerprint {
                return Err(app_error(Code::IdempotencyConflict));
            }
            return receipt(command, prior.accepted_revision, fingerprint)
                .map(|value| (value, false));
        }
        let previous = record.preview.clone();
        mutate(&mut record.preview)?;
        record.preview.operations.insert(
            command.operation(),
            super::super::super::PreviewOperationRecord {
                fingerprint,
                accepted_revision: command.expected_revision(),
                result_sequence: record
                    .preview
                    .page
                    .as_ref()
                    .map_or(0, WorkbenchResultPage::result_revision),
                completed_sequence: 0,
            },
        );
        if super::super::super::persist_record(&self.inner.directory, record).is_err() {
            record.preview = previous;
            return Err(app_error(Code::Backpressure));
        }
        receipt(command, command.expected_revision(), fingerprint).map(|value| (value, true))
    }

    pub(super) fn update_launch(
        &self,
        run: RunId,
        launch: ControlOperationId,
        update: impl FnOnce(&WorkbenchLaunchResult) -> Result<WorkbenchLaunchResult, AppProtocolError>,
    ) -> Result<(), AppProtocolError> {
        let mut records = self.inner.records.write().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get_mut(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let previous = record.preview.clone();
        mutate_launch(&mut record.preview, launch, update)?;
        if super::super::super::persist_record(&self.inner.directory, record).is_err() {
            record.preview = previous;
            return Err(app_error(Code::Backpressure));
        }
        Ok(())
    }
}

pub(super) fn replace_page(
    preview: &mut super::super::super::PreviewAggregate,
    query: WorkbenchQuery,
    control_revision: u64,
    launches: Vec<WorkbenchLaunchResult>,
) -> Result<(), AppProtocolError> {
    let result_revision =
        preview.page.as_ref().map_or(1, |page| page.result_revision().saturating_add(1));
    preview.page = Some(WorkbenchResultPage::new(
        WorkbenchResultQuery::new(
            query,
            launches.first().map_or_else(
                || preview.page.as_ref().expect("existing page for empty mutation").query().run(),
                |launch| launch.profile().run(),
            ),
        ),
        control_revision,
        result_revision,
        capture_capability(),
        launches,
    )?);
    Ok(())
}

pub(super) fn mutate_launch(
    preview: &mut super::super::super::PreviewAggregate,
    launch: ControlOperationId,
    update: impl FnOnce(&WorkbenchLaunchResult) -> Result<WorkbenchLaunchResult, AppProtocolError>,
) -> Result<(), AppProtocolError> {
    let page = preview.page.as_ref().ok_or_else(|| app_error(Code::InvalidIdentifier))?;
    let mut launches = page.launches().to_vec();
    let index = launches
        .iter()
        .position(|value| value.launch() == launch)
        .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
    launches[index] = update(&launches[index])?;
    let query = page.query().query();
    let revision = page.control_revision();
    replace_page(preview, query, revision, launches)
}

pub(super) fn require_launch(
    preview: &super::super::super::PreviewAggregate,
    launch: ControlOperationId,
) -> Result<&WorkbenchLaunchResult, AppProtocolError> {
    preview
        .page
        .as_ref()
        .and_then(|page| page.launches().iter().find(|value| value.launch() == launch))
        .ok_or_else(|| app_error(Code::InvalidIdentifier))
}

pub(super) fn has_launch(
    preview: &super::super::super::PreviewAggregate,
    launch: ControlOperationId,
) -> bool {
    require_launch(preview, launch).is_ok()
}

pub(super) fn find_capture(
    preview: &super::super::super::PreviewAggregate,
    capture: ControlOperationId,
) -> Result<(ControlOperationId, (u32, u32)), AppProtocolError> {
    preview
        .page
        .as_ref()
        .into_iter()
        .flat_map(WorkbenchResultPage::launches)
        .find_map(|launch| {
            launch
                .captures()
                .iter()
                .find(|value| {
                    value.operation() == capture && value.state() == WorkbenchCaptureState::Captured
                })
                .and_then(|value| {
                    value.dimensions().map(|dimensions| (launch.launch(), dimensions))
                })
        })
        .ok_or_else(|| app_error(Code::InvalidIdentifier))
}

pub(super) fn rebuild_launch(
    current: &WorkbenchLaunchResult,
    process: Option<peritus_types::ProcessId>,
    state: WorkbenchLaunchState,
    ready: bool,
) -> Result<WorkbenchLaunchResult, AppProtocolError> {
    WorkbenchLaunchResult::new(
        current.launch(),
        current.profile().clone(),
        process,
        state,
        ready,
        current.interactions().to_vec(),
        current.captures().to_vec(),
        current.feedback().to_vec(),
        current.behavior_checks(),
        current.stdout_digest(),
        current.exit_code(),
    )
}

pub(super) fn rebuild_launch_with(
    current: &WorkbenchLaunchResult,
    interactions: Vec<WorkbenchInteractionReceipt>,
    captures: Vec<WorkbenchCaptureReceipt>,
    feedback: Vec<WorkbenchArtifactFeedback>,
    behavior_checks: u32,
) -> Result<WorkbenchLaunchResult, AppProtocolError> {
    WorkbenchLaunchResult::new(
        current.launch(),
        current.profile().clone(),
        current.process(),
        current.state(),
        current.ready(),
        interactions,
        captures,
        feedback,
        behavior_checks,
        current.stdout_digest(),
        current.exit_code(),
    )
}

pub(super) fn terminal_launch(
    current: &WorkbenchLaunchResult,
    observation: &PreviewObservation,
) -> Result<WorkbenchLaunchResult, AppProtocolError> {
    WorkbenchLaunchResult::new(
        current.launch(),
        current.profile().clone(),
        current.process(),
        launch_state(observation.state()),
        current.ready(),
        current.interactions().to_vec(),
        current.captures().to_vec(),
        current.feedback().to_vec(),
        current.behavior_checks(),
        Some(peritus_codec::sha256(observation.stdout().as_bytes())),
        observation.exit_code().and_then(|value| u32::try_from(value).ok()),
    )
}
