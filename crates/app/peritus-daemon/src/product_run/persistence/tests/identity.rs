use super::*;

#[test]
fn malformed_legacy_projection_is_quarantined_without_blocking_startup() {
    let state = tempfile::tempdir().expect("state");
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).expect("run directory");
    let corrupt = directory.join("broken.json");
    fs::write(&corrupt, b"{not-json").expect("corrupt projection");

    let records = load_records(&directory).expect("healthy startup");

    assert!(records.is_empty());
    assert!(!corrupt.exists());
    assert_eq!(
        fs::read(directory.join(".quarantine/broken.json")).expect("quarantined bytes"),
        b"{not-json"
    );
}

#[test]
fn misnamed_legacy_projection_is_quarantined_instead_of_aliasing_another_run() {
    let state = tempfile::tempdir().expect("state");
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).expect("run directory");
    let path = directory.join("ffffffffffffffffffffffffffffffff.json");
    fs::write(
        &path,
        br#"{
            "run_id":"01010101010101010101010101010101",
            "workspace_id":"02020202020202020202020202020202",
            "writer":"03030303030303030303030303030303",
            "reviewer":"04040404040404040404040404040404",
            "fixer":"05050505050505050505050505050505",
            "phase":8,
            "cycle":1,
            "task":"inspect",
            "status":"failed",
            "diff":"",
            "gates":"",
            "review":"",
            "summary":"retained"
        }"#,
    )
    .expect("misnamed projection");

    let records = load_records(&directory).expect("healthy startup");

    assert!(records.is_empty());
    assert!(!path.exists());
    assert!(directory.join(".quarantine/ffffffffffffffffffffffffffffffff.json").is_file());
}
