use crate::{
    ControlOperationId, ConversationId, WorkbenchArtifactFeedback, WorkbenchArtifactRegion,
    WorkbenchBuildIdentity, WorkbenchCaptureCapability, WorkbenchCaptureReceipt,
    WorkbenchCaptureState, WorkbenchCaptureTarget, WorkbenchInputText, WorkbenchInteractionReceipt,
    WorkbenchLaunchProfile, WorkbenchLaunchResult, WorkbenchLaunchSource,
    WorkbenchLaunchSourceKind, WorkbenchLaunchState, WorkbenchLaunchText, WorkbenchQuery,
    WorkbenchResultPage, WorkbenchResultQuery, WorkbenchReviewFeedback,
    decode_workbench_result_value, encode_workbench_result_value,
};
use peritus_types::{ArtifactId, ProcessId, RunId, Sha256Digest, WorkspaceId};

fn text(value: &str) -> WorkbenchLaunchText {
    WorkbenchLaunchText::new(value.to_owned()).expect("bounded launch text")
}

#[test]
fn complete_result_page_round_trips_as_a_persistence_value()
-> Result<(), Box<dyn std::error::Error>> {
    let run = RunId::new([1; 16]).expect("run");
    let launch = ControlOperationId::new([2; 16]).expect("launch");
    let capture = ControlOperationId::new([3; 16]).expect("capture");
    let profile = WorkbenchLaunchProfile::new(
        run,
        text("python3"),
        vec![text("preview.py")],
        text("examples"),
        vec![text("DISPLAY")],
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::PlainFolderFile,
            text("examples/preview.py"),
            Sha256Digest::new([4; 32]),
        ),
        Some(WorkbenchBuildIdentity::new(text("target/preview"), Sha256Digest::new([5; 32]))),
        2_000,
        20_000,
        true,
    )?;
    let target = WorkbenchCaptureTarget::x11_window(0x2a)?;
    let result = WorkbenchLaunchResult::new(
        launch,
        profile,
        Some(ProcessId::new([6; 16]).expect("process")),
        WorkbenchLaunchState::Stopped,
        true,
        vec![WorkbenchInteractionReceipt::new(
            ControlOperationId::new([7; 16]).expect("interaction"),
            Sha256Digest::new([8; 32]),
            true,
        )],
        vec![WorkbenchCaptureReceipt::new(
            capture,
            WorkbenchCaptureState::Captured,
            target,
            Some(ArtifactId::new([9; 16]).expect("artifact")),
            Some(Sha256Digest::new([10; 32])),
            Some((320, 200)),
            Some(1_700_000_000_000),
            text("selected window captured"),
        )?],
        vec![WorkbenchArtifactFeedback::new(
            ControlOperationId::new([11; 16]).expect("feedback"),
            capture,
            WorkbenchReviewFeedback::RequestRevision,
            WorkbenchInputText::new("Move the control right".to_owned())?,
            Some(WorkbenchArtifactRegion::new(10, 20, 30, 40)?),
        )],
        1,
        Some(Sha256Digest::new([12; 32])),
        Some(0),
    )?;
    let page = WorkbenchResultPage::new(
        WorkbenchResultQuery::new(
            WorkbenchQuery::new(
                ConversationId::new([13; 16]).expect("conversation"),
                WorkspaceId::new([14; 16]).expect("workspace"),
            ),
            run,
        ),
        9,
        6,
        WorkbenchCaptureCapability::X11SelectedWindow,
        vec![result],
    )?;

    let encoded = encode_workbench_result_value(&page)?;
    assert_eq!(decode_workbench_result_value(&encoded)?, page);
    Ok(())
}
