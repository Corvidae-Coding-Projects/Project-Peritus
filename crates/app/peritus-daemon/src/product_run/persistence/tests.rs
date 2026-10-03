use super::*;

#[path = "tests/identity.rs"]
mod identity;
#[path = "tests/record_format.rs"]
mod record_format;

#[test]
fn ungoverned_workbench_projection_is_quarantined_without_blocking_startup() {
    let state = tempfile::tempdir().expect("state");
    let root = state.path().join("workbench-v1");
    let directory = root.join("runs");
    fs::create_dir_all(&directory).expect("run directory");
    let orphan = directory.join("orphan.json");
    fs::write(&orphan, b"retained evidence").expect("orphan projection");

    let records = load_workbench_records(&root, None).expect("healthy startup");

    assert!(records.is_empty());
    assert!(!orphan.exists());
    assert_eq!(
        fs::read(directory.join(".quarantine/orphan.json")).expect("quarantined bytes"),
        b"retained evidence"
    );
}
