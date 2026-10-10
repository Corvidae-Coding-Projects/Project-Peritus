use super::*;
use crate::{ControlOperationId, WorkbenchInputText, WorkbenchReviewFeedback};
use peritus_types::{ArtifactId, ProcessId, RunId, Sha256Digest};

fn text(value: &str) -> WorkbenchLaunchText {
    WorkbenchLaunchText::new(value.to_owned()).expect("text")
}

#[test]
fn evidence_classes_remain_independent() {
    let profile = WorkbenchLaunchProfile::new(
        RunId::new([1; 16]).unwrap(),
        text("python3"),
        vec![text("sample.pyc")],
        text("."),
        vec![text("DISPLAY")],
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::PlainFolderFile,
            text("sample.py"),
            Sha256Digest::new([2; 32]),
        ),
        Some(WorkbenchBuildIdentity::new(text("sample.pyc"), Sha256Digest::new([3; 32]))),
        Some(2_000),
        Some(10_000),
        true,
    )
    .unwrap();
    let result = WorkbenchLaunchResult::new(
        ControlOperationId::new([4; 16]).unwrap(),
        profile,
        Some(ProcessId::new([5; 16]).unwrap()),
        WorkbenchLaunchState::Running,
        true,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        None,
        None,
    )
    .unwrap();
    let evidence = result.evidence();
    assert!(evidence.built());
    assert!(evidence.launched());
    assert!(!evidence.captured());
    assert!(!evidence.behavior_checked());
    assert!(!evidence.human_reviewed());
}

#[test]
fn launch_profile_accepts_a_wall_horizon_above_ten_minutes() {
    let profile = WorkbenchLaunchProfile::new(
        RunId::new([1; 16]).unwrap(),
        text("python3"),
        Vec::new(),
        text("."),
        Vec::new(),
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::PlainFolderFile,
            text("sample.py"),
            Sha256Digest::new([2; 32]),
        ),
        None,
        Some(2_000),
        Some(1_000_000),
        false,
    )
    .expect("positive caller-selected wall horizon");

    assert_eq!(profile.wall_millis(), Some(1_000_000));
}

#[test]
fn launch_profile_accepts_collections_beyond_the_old_limits() {
    let arguments = (0..257).map(|index| text(&format!("argument-{index}"))).collect();
    let environment = (0..65).map(|index| text(&format!("VARIABLE_{index}"))).collect();
    let profile = WorkbenchLaunchProfile::new(
        RunId::new([1; 16]).unwrap(),
        text("python3"),
        arguments,
        text("."),
        environment,
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::PlainFolderFile,
            text("sample.py"),
            Sha256Digest::new([2; 32]),
        ),
        None,
        Some(2_000),
        Some(10_000),
        false,
    )
    .expect("protocol-representable launch profile");

    assert_eq!(profile.arguments().len(), 257);
    assert_eq!(profile.environment().len(), 65);
}

#[test]
fn denied_capture_cannot_carry_fabricated_artifact_facts() {
    let target = WorkbenchCaptureTarget::x11_window(42).unwrap();
    assert!(
        WorkbenchCaptureReceipt::new(
            ControlOperationId::new([1; 16]).unwrap(),
            WorkbenchCaptureState::Denied,
            target,
            Some(ArtifactId::new([2; 16]).unwrap()),
            Some(Sha256Digest::new([3; 32])),
            Some((10, 10)),
            Some(1),
            text("denied"),
        )
        .is_err()
    );
}

#[test]
fn launch_evidence_grows_past_the_old_preview_totals() {
    let run = RunId::new([1; 16]).unwrap();
    let profile = plain_python_profile(run);
    let operation = |index: u16| {
        let mut bytes = [0; 16];
        bytes[14..].copy_from_slice(&index.saturating_add(1).to_be_bytes());
        ControlOperationId::new(bytes).unwrap()
    };
    let target = WorkbenchCaptureTarget::x11_window(42).unwrap();
    let interactions = (0..257)
        .map(|index| {
            WorkbenchInteractionReceipt::new(operation(index), Sha256Digest::new([3; 32]), true)
        })
        .collect();
    let captures = (0..65)
        .map(|index| {
            WorkbenchCaptureReceipt::new(
                operation(index),
                WorkbenchCaptureState::Denied,
                target,
                None,
                None,
                None,
                None,
                text("capture denied"),
            )
            .unwrap()
        })
        .collect();
    let feedback = (0..257)
        .map(|index| {
            WorkbenchArtifactFeedback::new(
                operation(index),
                operation(0),
                WorkbenchReviewFeedback::RequestRevision,
                WorkbenchInputText::new(format!("feedback {index}")).unwrap(),
                None,
            )
        })
        .collect();

    let result = WorkbenchLaunchResult::new(
        operation(0),
        profile.clone(),
        None,
        WorkbenchLaunchState::Accepted,
        false,
        interactions,
        captures,
        feedback,
        0,
        None,
        None,
    )
    .expect("evidence beyond the old totals");
    let launches = (0..17)
        .map(|index| {
            WorkbenchLaunchResult::new(
                operation(index),
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
            .unwrap()
        })
        .collect();
    let page = WorkbenchResultPage::new(
        WorkbenchResultQuery::new(
            crate::WorkbenchQuery::new(
                crate::ConversationId::new([4; 16]).unwrap(),
                peritus_types::WorkspaceId::new([5; 16]).unwrap(),
            ),
            run,
        ),
        1,
        1,
        WorkbenchCaptureCapability::X11SelectedWindow,
        launches,
    )
    .expect("launch history beyond the old total");

    assert_eq!(result.interactions().len(), 257);
    assert_eq!(result.captures().len(), 65);
    assert_eq!(result.feedback().len(), 257);
    assert_eq!(page.launches().len(), 17);
}

fn plain_python_profile(run: RunId) -> WorkbenchLaunchProfile {
    WorkbenchLaunchProfile::new(
        run,
        text("python3"),
        Vec::new(),
        text("."),
        Vec::new(),
        WorkbenchLaunchSource::new(
            WorkbenchLaunchSourceKind::PlainFolderFile,
            text("sample.py"),
            Sha256Digest::new([2; 32]),
        ),
        None,
        Some(2_000),
        Some(10_000),
        true,
    )
    .expect("plain Python fixture")
}
