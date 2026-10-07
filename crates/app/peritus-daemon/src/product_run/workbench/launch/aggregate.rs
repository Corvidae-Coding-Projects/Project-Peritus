//! Durable preview aggregate admission and immutable result rebuilding.

use super::*;
use crate::product_run::{
    ProductRunServiceError,
    publication::{MutationDisposition, RunIdentitySnapshot, RunMutationKind},
};

impl ProductRunService {
    pub(in crate::product_run::workbench) fn resolve_preview_receipt(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let result = (|| {
            self.preview_scope(actor, command)?;
            let run = self.preview_run(command)?;
            self.validate_preview_binding(run, command.query())?;
            let fingerprint = command.fingerprint()?;
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            let prior = record
                .preview
                .operations
                .get(&command.operation())
                .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            if prior.fingerprint != fingerprint {
                return Err(app_error(Code::IdempotencyConflict));
            }
            receipt(command, prior.accepted_revision, fingerprint)
        })();
        result.map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }

    pub(super) fn admit_preview(
        &self,
        command: &WorkbenchCommand,
        run: RunId,
        mutate: impl FnOnce(&mut super::super::super::PreviewAggregate) -> Result<(), AppProtocolError>,
    ) -> Result<(WorkbenchReceipt, bool), AppProtocolError> {
        let identity = self.capture_run_identity(run).map_err(preview_service_error)?;
        self.admit_preview_with_identity(command, run, &identity, mutate)
    }

    pub(super) fn admit_preview_with_identity(
        &self,
        command: &WorkbenchCommand,
        run: RunId,
        identity: &RunIdentitySnapshot,
        mutate: impl FnOnce(&mut super::super::super::PreviewAggregate) -> Result<(), AppProtocolError>,
    ) -> Result<(WorkbenchReceipt, bool), AppProtocolError> {
        let fingerprint = command.fingerprint().map_err(|_| app_error(Code::MalformedFrame))?;
        if identity.run != run {
            return Err(app_error(Code::InvalidIdentifier));
        }
        let mut input = b"peritus-product-run-preview-admission-v1\0".to_vec();
        input.extend_from_slice(run.as_bytes());
        input.extend_from_slice(command.operation().as_bytes());
        input.extend_from_slice(fingerprint.as_bytes());
        input.extend_from_slice(&command.expected_revision().to_be_bytes());
        let ((accepted_revision, admitted), ticket) = self
            .mutate_run(
                run,
                Some(&identity.cancelled),
                RunMutationKind::PreviewAdmission,
                peritus_codec::sha256(&input),
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.interaction.workbench != identity.start
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    if let Some(prior) = record.preview.operations.get(&command.operation()) {
                        if prior.fingerprint != fingerprint {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                        return Ok((prior.accepted_revision, false));
                    }
                    mutate(&mut record.preview).map_err(preview_mutation_error)?;
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
                    Ok((command.expected_revision(), true))
                },
            )
            .map_err(preview_service_error)?;
        self.await_run_durable(ticket).map_err(preview_service_error)?;
        receipt(command, accepted_revision, fingerprint).map(|value| (value, admitted))
    }

    pub(super) fn update_launch(
        &self,
        identity: &RunIdentitySnapshot,
        operation: ControlOperationId,
        launch: ControlOperationId,
        update: impl FnOnce(
            &WorkbenchLaunchResult,
        ) -> Result<Option<WorkbenchLaunchResult>, AppProtocolError>,
    ) -> Result<(), AppProtocolError> {
        let run = identity.run;
        let mut input = b"peritus-product-run-preview-launch-update-v1\0".to_vec();
        input.extend_from_slice(run.as_bytes());
        input.extend_from_slice(operation.as_bytes());
        input.extend_from_slice(launch.as_bytes());
        let (_, ticket) = self
            .mutate_run(
                run,
                Some(&identity.cancelled),
                RunMutationKind::PreviewObservation,
                peritus_codec::sha256(&input),
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.interaction.workbench != identity.start
                        || !record.preview.operations.contains_key(&operation)
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    let current = require_launch(&record.preview, launch)
                        .map_err(preview_mutation_error)?;
                    let Some(updated) = update(current).map_err(preview_mutation_error)? else {
                        return Ok(());
                    };
                    mutate_launch(&mut record.preview, launch, |_| Ok(updated))
                        .map_err(preview_mutation_error)
                },
            )
            .map_err(preview_service_error)?;
        self.await_run_durable(ticket).map_err(preview_service_error)
    }
}

pub(super) fn preview_mutation_error(error: AppProtocolError) -> ProductRunServiceError {
    match error.code() {
        Code::InvalidIdentifier => ProductRunServiceError::NotFound,
        Code::IdempotencyConflict | Code::StaleRevision => ProductRunServiceError::InvalidState,
        Code::MalformedFrame => ProductRunServiceError::InvalidMessage,
        _ => ProductRunServiceError::Unavailable,
    }
}

pub(super) fn preview_service_error(error: ProductRunServiceError) -> AppProtocolError {
    match error {
        ProductRunServiceError::NotFound => app_error(Code::InvalidIdentifier),
        ProductRunServiceError::Duplicate | ProductRunServiceError::InvalidState => {
            app_error(Code::IdempotencyConflict)
        }
        ProductRunServiceError::InvalidMessage => app_error(Code::MalformedFrame),
        ProductRunServiceError::Control(ControlError::StaleRevision) => {
            app_error(Code::StaleRevision)
        }
        ProductRunServiceError::Control(ControlError::IdempotencyConflict) => {
            app_error(Code::IdempotencyConflict)
        }
        ProductRunServiceError::Control(ControlError::NotFound) => {
            app_error(Code::InvalidIdentifier)
        }
        _ => app_error(Code::Backpressure),
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
