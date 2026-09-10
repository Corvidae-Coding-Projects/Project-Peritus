//! Durable admission and daemon-owned preview launch, interaction, capture, and inspection.

use std::{
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::GenericImageView as _;
use peritus_app_protocol::{
    AppErrorCode as Code, AppProtocolError, AppResponsePayload, ArtifactCancellation,
    ArtifactChunk, ArtifactCompletion, ArtifactMetadata, CanonicalMediaType, ControlOperationId,
    CorrelationId, TransferId, WorkbenchArtifactFeedback, WorkbenchArtifactRegion,
    WorkbenchCaptureCapability, WorkbenchCaptureConsent, WorkbenchCaptureReceipt,
    WorkbenchCaptureState, WorkbenchCaptureTarget, WorkbenchCommand, WorkbenchIntent,
    WorkbenchInteractionReceipt, WorkbenchLaunchProfile, WorkbenchLaunchResult,
    WorkbenchLaunchSourceKind, WorkbenchLaunchState, WorkbenchLaunchText, WorkbenchQuery,
    WorkbenchReceipt, WorkbenchResultPage, WorkbenchResultQuery,
};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ConversationId, GoalCriterionKind, OperationId,
};
use peritus_product_runner::{
    CommandRuntime, PreviewCommand, PreviewObservation, PreviewProcessState, ProductRunner,
};
use peritus_types::{ActorId, ArtifactId, RunId, SessionId, Sha256Digest};

use super::{ProductRunService, error_value};
use crate::{AuthorityHandle, artifact::ArtifactScope};

mod aggregate;
mod capture;
mod evidence;
mod process;

use aggregate::{
    find_capture, has_launch, mutate_launch, rebuild_launch, rebuild_launch_with, replace_page,
    require_launch, terminal_launch,
};
use capture::{capture_capability_for, hex, receipt, region_within};
use evidence::{active_launch_run, capture_capability, preview_launches, resolve_workspace_path};
use process::{PreviewTarget, app_error, launch_state, text};

const MAX_CAPTURE_BYTES: u64 = 16 * 1_024 * 1_024;

impl ProductRunService {
    pub(crate) async fn workbench_preview_command(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        correlation: CorrelationId,
        maximum_chunk_bytes: usize,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let result = self
            .apply_preview_command(
                authority,
                actor,
                session,
                correlation,
                maximum_chunk_bytes,
                command,
            )
            .await;
        result.map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }

    async fn apply_preview_command(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        correlation: CorrelationId,
        maximum_chunk_bytes: usize,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let scope = self.image_scope(actor, command.query(), command.expected_revision())?;
        let run = self.preview_run(command)?;
        self.validate_preview_binding(run, command.query())?;
        let required = super::super::permissions::command_permissions(command.intent());
        if !required.is_empty() {
            self.require_run_permissions(run, required).map_err(error_value)?;
        }
        if !matches!(command.intent(), WorkbenchIntent::CapturePreview(_)) {
            return self.apply_preview_local_command(command);
        }
        match command.intent() {
            WorkbenchIntent::CapturePreview(request) => {
                self.capture_preview(
                    authority,
                    actor,
                    session,
                    correlation,
                    maximum_chunk_bytes,
                    scope,
                    run,
                    command,
                    *request,
                )
                .await
            }
            _ => Err(app_error(Code::MalformedFrame)),
        }
    }

    pub(in crate::product_run) fn workbench_preview_local_command(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let result = self
            .image_scope(actor, command.query(), command.expected_revision())
            .and_then(|_| self.apply_preview_local_command(command));
        result.map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }

    fn apply_preview_local_command(
        &self,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let run = self.preview_run(command)?;
        self.validate_preview_binding(run, command.query())?;
        let required = super::super::permissions::command_permissions(command.intent());
        if !required.is_empty() {
            self.require_run_permissions(run, required).map_err(error_value)?;
        }
        match command.intent() {
            WorkbenchIntent::StartPreview(profile) => self.start_preview(command, profile),
            WorkbenchIntent::InteractPreview { launch, input } => {
                self.interact_preview(run, command, *launch, input.bytes())
            }
            WorkbenchIntent::StopPreview { launch } => self.stop_preview(run, command, *launch),
            WorkbenchIntent::CheckPreviewBehavior { launch, observed, .. } => {
                self.check_preview_behavior(run, command, *launch, observed)
            }
            WorkbenchIntent::AddArtifactFeedback { capture, feedback, message, region } => self
                .add_artifact_feedback(run, command, *capture, *feedback, message.clone(), *region),
            _ => Err(app_error(Code::MalformedFrame)),
        }
    }

    pub(crate) fn workbench_result(
        &self,
        actor: ActorId,
        query: WorkbenchResultQuery,
    ) -> AppResponsePayload {
        self.result_page(actor, query)
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchResult)
    }

    pub(in crate::product_run) fn result_page(
        &self,
        actor: ActorId,
        query: WorkbenchResultQuery,
    ) -> Result<WorkbenchResultPage, AppProtocolError> {
        self.control_workspace(query.query()).map_err(error_value)?;
        let control_revision = self
            .with_controls(false, |store| {
                let conversation = ConversationId::new(query.query().conversation().into_bytes())?;
                let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != query.query().workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                Ok(record.revision())
            })
            .map_err(error_value)?;
        self.validate_preview_binding(query.run(), query.query())?;
        self.refresh_previews(query.run())?;
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get(&query.run()).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let (result_revision, launches) = record.preview.page.as_ref().map_or_else(
            || (0, Vec::new()),
            |page| (page.result_revision(), page.launches().to_vec()),
        );
        WorkbenchResultPage::new(
            query,
            control_revision,
            result_revision,
            capture_capability_for(&self.inner.preview_capture),
            launches,
        )
        .map_err(|_| app_error(Code::MalformedFrame))
    }

    pub(in crate::product_run) fn start_preview(
        &self,
        command: &WorkbenchCommand,
        profile: &WorkbenchLaunchProfile,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        if profile.run() != self.preview_run(command)? {
            return Err(app_error(Code::MalformedFrame));
        }
        let (workspace, direct) = self.verify_profile(command.query(), profile)?;
        let (receipt, admitted) = self.admit_preview(command, profile.run(), |preview| {
            let mut launches = preview_launches(preview);
            if launches.len() >= peritus_app_protocol::MAX_WORKBENCH_LAUNCHES {
                return Err(app_error(Code::LimitExceeded));
            }
            launches.push(
                WorkbenchLaunchResult::new(
                    command.operation(),
                    profile.clone(),
                    None,
                    WorkbenchLaunchState::Accepted,
                    false,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    0,
                    None,
                    None,
                )
                .map_err(|_| app_error(Code::MalformedFrame))?,
            );
            replace_page(preview, command.query(), command.expected_revision(), launches)
        })?;
        if !admitted {
            return Ok(receipt);
        }
        let launch_result =
            self.launch_owned_preview(profile, workspace, direct, command.operation());
        match launch_result {
            Ok((runtime, launch, observation)) => {
                self.inner
                    .preview_processes
                    .lock()
                    .map_err(|_| app_error(Code::Backpressure))?
                    .insert(
                        command.operation(),
                        super::super::PreviewProcess { runtime, launch: launch.clone() },
                    );
                self.store_observation(profile.run(), command.operation(), &launch, &observation)?;
            }
            Err(_) => {
                self.update_launch(profile.run(), command.operation(), |current| {
                    rebuild_launch(current, None, WorkbenchLaunchState::Failed, false)
                })?;
            }
        }
        Ok(receipt)
    }
}
