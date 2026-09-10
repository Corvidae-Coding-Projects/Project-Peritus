use super::*;
use crate::ControlOperationId;
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
        2_000,
        10_000,
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
