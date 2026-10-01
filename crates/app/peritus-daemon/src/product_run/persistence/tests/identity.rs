use super::*;

#[test]
fn malformed_projection_is_quarantined_without_blocking_startup() {
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
fn previous_format_projection_is_quarantined_without_migration() {
    let state = tempfile::tempdir().expect("state");
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).expect("run directory");
    let path = directory.join("01010101010101010101010101010101.json");
    fs::write(
        &path,
        br#"{
            "format_version":2,
            "goal_resume":null,
            "interaction":null,
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
            "summary":"retained",
            "user_cancelled":false,
            "finding_state":"",
            "deliverable":null,
            "messages":[],
            "conversation_revision":0,
            "progress":{"started_unix_millis":0,"last_effect_unix_millis":0,"model_requests":0,"tool_calls":0,"retries":0,"provider_failovers":0,"compactions":0,"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"total_tokens":0,"provider_cost_microunits":0,"usage_observations":0,"workspace_bytes":0,"workspace_growth_bytes":0,"peak_rss_bytes":0},
            "checkpoint":null,
            "settlement_cause":null,
            "resume_state":null,
            "remaining_work":[],
            "interruption_cause":"",
            "candidate_actionable":false,
            "task_baseline_required":false,
            "task_baseline":null,
            "preview_page":null,
            "preview_operations":[],
            "preview_outputs":[]
        }"#,
    )
    .expect("misnamed projection");

    let records = load_records(&directory).expect("healthy startup");

    assert!(records.is_empty());
    assert!(!path.exists());
    assert!(directory.join(".quarantine/01010101010101010101010101010101.json").is_file());
}
