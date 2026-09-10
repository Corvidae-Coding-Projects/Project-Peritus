//! Exact launch, interaction, selected-window capture, and independent evidence frames.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    CorrelationId, RequestId, WorkbenchArtifactFeedback, WorkbenchArtifactRegion,
    WorkbenchBuildIdentity, WorkbenchCaptureCapability, WorkbenchCaptureConsent,
    WorkbenchCaptureReceipt, WorkbenchCaptureRequest, WorkbenchCaptureState,
    WorkbenchCaptureTarget, WorkbenchCommand, WorkbenchInputText, WorkbenchIntent,
    WorkbenchInteractionReceipt, WorkbenchLaunchProfile, WorkbenchLaunchResult,
    WorkbenchLaunchSource, WorkbenchLaunchSourceKind, WorkbenchLaunchState, WorkbenchLaunchText,
    WorkbenchPreviewInput, WorkbenchQuery, WorkbenchReceipt, WorkbenchResultPage,
    WorkbenchResultQuery, WorkbenchReviewFeedback,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ArtifactId, ProcessId, RunId, Sha256Digest, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query = WorkbenchQuery::new(id(41, ConversationId::new), id(32, WorkspaceId::new));
    let run = id(70, RunId::new);
    let launch = id(71, ControlOperationId::new);
    let capture = id(72, ControlOperationId::new);
    let target = WorkbenchCaptureTarget::x11_window(0x440_001).expect("window target");
    let profile = profile(run);
    let mut cases = command_cases(query, launch, capture, target, &profile, limits)?;
    let result_query = WorkbenchResultQuery::new(query, run);
    cases.push(encoded(
        "minimal-workbench-result-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchResult(result_query)),
        limits,
    )?);
    cases.push(encoded(
        "realistic-workbench-preview-receipt",
        FixtureClass::Realistic,
        &response(AppResponsePayload::WorkbenchReceipt(
            WorkbenchReceipt::new(launch, query, 9, Sha256Digest::new([77; 32])).expect("receipt"),
        )),
        limits,
    )?);
    cases.push(encoded(
        "realistic-workbench-result",
        FixtureClass::Realistic,
        &response(AppResponsePayload::WorkbenchResult(result_page(
            result_query,
            launch,
            capture,
            target,
            profile,
        ))),
        limits,
    )?);
    Ok(cases)
}

fn command_cases(
    query: WorkbenchQuery,
    launch: ControlOperationId,
    capture: ControlOperationId,
    target: WorkbenchCaptureTarget,
    profile: &WorkbenchLaunchProfile,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let commands = [
        (
            "minimal-workbench-preview-start",
            WorkbenchCommand::new(launch, query, 9, WorkbenchIntent::StartPreview(profile.clone())),
        ),
        (
            "minimal-workbench-preview-interact",
            WorkbenchCommand::new(
                id(73, ControlOperationId::new),
                query,
                9,
                WorkbenchIntent::InteractPreview {
                    launch,
                    input: WorkbenchPreviewInput::new(b"MOVE_RIGHT\n".to_vec()).expect("input"),
                },
            ),
        ),
        (
            "minimal-workbench-preview-capture",
            WorkbenchCommand::new(
                capture,
                query,
                9,
                WorkbenchIntent::CapturePreview(WorkbenchCaptureRequest::new(
                    launch,
                    target,
                    WorkbenchCaptureConsent::Granted,
                )),
            ),
        ),
        (
            "minimal-workbench-preview-stop",
            WorkbenchCommand::new(
                id(74, ControlOperationId::new),
                query,
                9,
                WorkbenchIntent::StopPreview { launch },
            ),
        ),
        (
            "minimal-workbench-preview-check",
            WorkbenchCommand::new(
                id(75, ControlOperationId::new),
                query,
                9,
                WorkbenchIntent::CheckPreviewBehavior {
                    launch,
                    observed: text("OBSERVED MOVE_RIGHT state=1"),
                    note: WorkbenchInputText::new("moved right after explicit input".to_owned())
                        .expect("note"),
                },
            ),
        ),
        (
            "minimal-workbench-artifact-feedback",
            WorkbenchCommand::new(
                id(76, ControlOperationId::new),
                query,
                9,
                WorkbenchIntent::AddArtifactFeedback {
                    capture,
                    feedback: WorkbenchReviewFeedback::RequestRevision,
                    message: WorkbenchInputText::new("Move the control right".to_owned())
                        .expect("feedback"),
                    region: Some(
                        WorkbenchArtifactRegion::new(10, 20, 30, 40).expect("feedback region"),
                    ),
                },
            ),
        ),
    ];
    commands
        .into_iter()
        .map(|(name, command)| {
            encoded(
                name,
                FixtureClass::Minimal,
                &request(AppRequestPayload::WorkbenchCommand(command)),
                limits,
            )
        })
        .collect()
}

fn profile(run: RunId) -> WorkbenchLaunchProfile {
    WorkbenchLaunchProfile::new(
        run,
        text("python3"),
        vec![text("preview.py")],
        text("examples"),
        vec![text("DISPLAY")],
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::ManagedCandidate,
            text("."),
            Sha256Digest::new([78; 32]),
        ),
        Some(WorkbenchBuildIdentity::new(text("target/preview"), Sha256Digest::new([79; 32]))),
        2_000,
        20_000,
        true,
    )
    .expect("launch profile")
}

fn result_page(
    query: WorkbenchResultQuery,
    launch: ControlOperationId,
    capture: ControlOperationId,
    target: WorkbenchCaptureTarget,
    profile: WorkbenchLaunchProfile,
) -> WorkbenchResultPage {
    let launch = WorkbenchLaunchResult::new(
        launch,
        profile,
        Some(id(80, ProcessId::new)),
        WorkbenchLaunchState::Stopped,
        true,
        vec![WorkbenchInteractionReceipt::new(
            id(81, ControlOperationId::new),
            Sha256Digest::new([82; 32]),
            true,
        )],
        vec![
            WorkbenchCaptureReceipt::new(
                capture,
                WorkbenchCaptureState::Captured,
                target,
                Some(id(83, ArtifactId::new)),
                Some(Sha256Digest::new([84; 32])),
                Some((640, 480)),
                Some(1_700_000_000_000),
                text("selected window captured"),
            )
            .expect("capture receipt"),
        ],
        vec![WorkbenchArtifactFeedback::new(
            id(85, ControlOperationId::new),
            capture,
            WorkbenchReviewFeedback::RequestRevision,
            WorkbenchInputText::new("Move the control right".to_owned()).expect("feedback"),
            Some(WorkbenchArtifactRegion::new(10, 20, 30, 40).expect("region")),
        )],
        1,
        Some(Sha256Digest::new([86; 32])),
        Some(0),
    )
    .expect("launch result");
    WorkbenchResultPage::new(
        query,
        9,
        6,
        WorkbenchCaptureCapability::X11SelectedWindow,
        vec![launch],
    )
    .expect("result page")
}

fn response(payload: AppResponsePayload) -> AppResponseEnvelope {
    AppResponseEnvelope::new(context(), id(10, RequestId::new), id(11, CorrelationId::new), payload)
}

fn text(value: &str) -> WorkbenchLaunchText {
    WorkbenchLaunchText::new(value.to_owned()).expect("launch text")
}
