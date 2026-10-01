use super::*;
use peritus_run_settlement::CandidateStage;

#[path = "tests/identity.rs"]
mod identity;

#[test]
fn canonical_record_rejects_omitted_current_fields() {
    let record = PersistedRecord {
        format_version: 2,
        goal_resume: None,
        interaction: None,
        run_id: "01010101010101010101010101010101".to_owned(),
        workspace_id: "02020202020202020202020202020202".to_owned(),
        writer: "03030303030303030303030303030303".to_owned(),
        reviewer: "04040404040404040404040404040404".to_owned(),
        fixer: "05050505050505050505050505050505".to_owned(),
        phase: ProductRunPhase::Complete.tag(),
        cycle: 1,
        task: "build tetris".to_owned(),
        status: "qualified".to_owned(),
        diff: "diff --git".to_owned(),
        gates: "cargo test: PASS".to_owned(),
        review: "clean".to_owned(),
        summary: "candidate retained".to_owned(),
        user_cancelled: false,
        finding_state: String::new(),
        deliverable: Some(PersistedDeliverable {
            workspace_path: "/managed/tetris".to_owned(),
            changed_paths: vec!["src/main.rs".to_owned()],
            successful_commands: vec!["cargo test".to_owned()],
            run_instructions: "cargo run".to_owned(),
            qualification: CandidateStage::Qualified.tag(),
            accepted: false,
            commit_revision: String::new(),
            export_path: String::new(),
            discarded: false,
        }),
        messages: vec![PersistedMessage {
            role: ProductConversationRole::User.tag(),
            content: "build tetris".to_owned(),
        }],
        conversation_revision: 1,
        progress: PersistedProgress::default(),
        checkpoint: None,
        settlement_cause: None,
        resume_state: None,
        remaining_work: Vec::new(),
        interruption_cause: String::new(),
        candidate_actionable: true,
        task_baseline_required: true,
        task_baseline: None,
        preview_page: None,
        preview_operations: Vec::new(),
        preview_outputs: Vec::new(),
    };
    let canonical = serde_json::to_value(record).expect("canonical record JSON");

    for field in [
        "format_version",
        "conversation_revision",
        "candidate_actionable",
        "task_baseline_required",
    ] {
        let mut missing = canonical.clone();
        missing.as_object_mut().expect("record object").remove(field);
        assert!(
            serde_json::from_value::<PersistedRecord>(missing).is_err(),
            "omitted current field {field} must not acquire a compatibility default",
        );
    }

    let mut missing_qualification = canonical;
    missing_qualification["deliverable"]
        .as_object_mut()
        .expect("deliverable object")
        .remove("qualification");
    assert!(
        serde_json::from_value::<PersistedRecord>(missing_qualification).is_err(),
        "omitted qualification must not become an implicitly qualified candidate",
    );
}

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

#[test]
fn cancelled_recovery_record_does_not_become_automatically_resumable() {
    let json = r#"{
        "format_version":2,
        "run_id":"01010101010101010101010101010101",
        "workspace_id":"02020202020202020202020202020202",
        "writer":"03030303030303030303030303030303",
        "reviewer":"04040404040404040404040404040404",
        "fixer":"05050505050505050505050505050505",
        "phase":10,
        "cycle":1,
        "task":"build tetris",
        "status":"recovery required",
        "diff":"",
        "gates":"",
        "review":"",
        "summary":"interrupted",
        "user_cancelled":true,
        "finding_state":"",
        "deliverable":null,
        "messages":[{"role":1,"content":"build tetris"}],
        "conversation_revision":1,
        "progress":{"started_unix_millis":0,"last_effect_unix_millis":0,"model_requests":0,"tool_calls":0,"retries":0,"provider_failovers":0,"compactions":0,"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"total_tokens":0,"provider_cost_microunits":0,"usage_observations":0,"workspace_bytes":0,"workspace_growth_bytes":0,"peak_rss_bytes":0},
        "checkpoint":null,
        "settlement_cause":null,
        "resume_state":null,
        "remaining_work":[],
        "interruption_cause":"",
        "candidate_actionable":false,
        "task_baseline_required":false,
        "preview_page":null,
        "preview_operations":[],
        "preview_outputs":[]
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("recovery record");

    let record = persisted.into_record().expect("restored cancelled record");

    assert_eq!(record.snapshot.phase(), ProductRunPhase::Cancelled);
    assert!(record.user_cancelled);
}

#[test]
fn durable_finding_state_survives_record_restoration() {
    let json = r#"{
        "format_version":2,
        "run_id":"11111111111111111111111111111111",
        "workspace_id":"12121212121212121212121212121212",
        "writer":"13131313131313131313131313131313",
        "reviewer":"14141414141414141414141414141414",
        "fixer":"15151515151515151515151515151515",
        "phase":8,
        "cycle":2,
        "task":"build tetris",
        "status":"review interrupted",
        "diff":"diff --git",
        "gates":"cargo test: PASS",
        "review":"nested target finding",
        "summary":"implementation retained",
        "user_cancelled":false,
        "finding_state":"{\"cycle\":1,\"summary\":\"nested target finding\",\"findings\":[]}",
        "deliverable":null,
        "messages":[{"role":1,"content":"build tetris"}],
        "conversation_revision":1,
        "progress":{"started_unix_millis":0,"last_effect_unix_millis":0,"model_requests":0,"tool_calls":0,"retries":0,"provider_failovers":0,"compactions":0,"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"total_tokens":0,"provider_cost_microunits":0,"usage_observations":0,"workspace_bytes":0,"workspace_growth_bytes":0,"peak_rss_bytes":0},
        "checkpoint":null,
        "settlement_cause":null,
        "resume_state":null,
        "remaining_work":[],
        "interruption_cause":"",
        "candidate_actionable":false,
        "task_baseline_required":false,
        "preview_page":null,
        "preview_operations":[],
        "preview_outputs":[]
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("persisted record");
    let expected = persisted.finding_state.clone();

    let record = persisted.into_record().expect("restored record");

    assert_eq!(record.finding_state, expected);
}

#[test]
fn candidate_qualification_is_independent_from_user_disposition() {
    let json = r#"{
        "format_version":2,
        "run_id":"21212121212121212121212121212121",
        "workspace_id":"22222222222222222222222222222222",
        "writer":"23232323232323232323232323232323",
        "reviewer":"24242424242424242424242424242424",
        "fixer":"25252525252525252525252525252525",
        "phase":8,
        "cycle":2,
        "task":"build tetris",
        "status":"candidate available",
        "diff":"diff --git",
        "gates":"cargo test failed",
        "review":"review missing",
        "summary":"candidate retained",
        "user_cancelled":false,
        "finding_state":"",
        "deliverable":{
            "workspace_path":"/managed/tetris",
            "changed_paths":["src/main.rs"],
            "successful_commands":[],
            "run_instructions":"cargo run",
            "qualification":2,
            "accepted":true,
            "commit_revision":"",
            "export_path":"/tmp/tetris.patch",
            "discarded":false
        },
        "messages":[{"role":1,"content":"build tetris"}],
        "conversation_revision":1,
        "progress":{"started_unix_millis":0,"last_effect_unix_millis":0,"model_requests":0,"tool_calls":0,"retries":0,"provider_failovers":0,"compactions":0,"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"total_tokens":0,"provider_cost_microunits":0,"usage_observations":0,"workspace_bytes":0,"workspace_growth_bytes":0,"peak_rss_bytes":0},
        "checkpoint":null,
        "settlement_cause":null,
        "resume_state":null,
        "remaining_work":[],
        "interruption_cause":"",
        "candidate_actionable":true,
        "task_baseline_required":false,
        "preview_page":null,
        "preview_operations":[],
        "preview_outputs":[]
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("candidate record");
    let record = persisted.into_record().expect("restored record");
    let deliverable = record.snapshot.deliverable().expect("deliverable");

    assert_eq!(deliverable.qualification(), CandidateStage::Changed);
    assert!(deliverable.accepted());
    assert_eq!(deliverable.export_path(), "/tmp/tetris.patch");
}

#[test]
fn restart_restores_each_resumable_phase_and_preserves_completed_writer_state() {
    use peritus_run_settlement::{CandidateCheckpoint, CandidateIdentity, EvidenceStatus};
    use peritus_types::Sha256Digest;

    let run_id = RunId::new([0x41; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0x42; 16]).expect("workspace");
    let identity = CandidateIdentity::new(
        run_id,
        workspace_id,
        Sha256Digest::new([0x43; 32]),
        Sha256Digest::new([0x43; 32]),
        None,
        1,
        2,
    )
    .expect("identity");
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint");

    for (phase_tag, expected) in [
        (2, peritus_product_runner::ProductRunPhase::Writing),
        (3, peritus_product_runner::ProductRunPhase::Checking),
        (4, peritus_product_runner::ProductRunPhase::Checking),
        (5, peritus_product_runner::ProductRunPhase::Checking),
        (6, peritus_product_runner::ProductRunPhase::Checking),
        (7, peritus_product_runner::ProductRunPhase::Checking),
    ] {
        let resume = durable_resume_json(identity, phase_tag);
        let record = PersistedRecord {
            format_version: 2,
            interaction: None,
            goal_resume: None,
            task_baseline_required: false,
            task_baseline: None,
            run_id: hex(run_id.as_bytes()),
            workspace_id: hex(workspace_id.as_bytes()),
            writer: "44444444444444444444444444444444".to_owned(),
            reviewer: "45454545454545454545454545454545".to_owned(),
            fixer: "46464646464646464646464646464646".to_owned(),
            phase: ProductRunPhase::Writing.tag(),
            cycle: 1,
            task: "build tetris".to_owned(),
            status: "working".to_owned(),
            diff: "diff --git".to_owned(),
            gates: String::new(),
            review: String::new(),
            summary: "writer state retained".to_owned(),
            user_cancelled: false,
            finding_state: String::new(),
            deliverable: Some(PersistedDeliverable {
                workspace_path: "/managed/tetris".to_owned(),
                changed_paths: vec!["src/main.rs".to_owned()],
                successful_commands: Vec::new(),
                run_instructions: "cargo run".to_owned(),
                qualification: CandidateStage::Changed.tag(),
                accepted: false,
                commit_revision: String::new(),
                export_path: String::new(),
                discarded: false,
            }),
            messages: vec![PersistedMessage {
                role: ProductConversationRole::User.tag(),
                content: "build tetris".to_owned(),
            }],
            conversation_revision: 1,
            progress: PersistedProgress::default(),
            checkpoint: Some(PersistedCheckpoint::from_checkpoint(&checkpoint)),
            settlement_cause: None,
            resume_state: Some(resume),
            remaining_work: vec!["finish current phase".to_owned()],
            interruption_cause: "daemon restart".to_owned(),
            candidate_actionable: true,
            preview_page: None,
            preview_operations: Vec::new(),
            preview_outputs: Vec::new(),
        }
        .into_record()
        .expect("restored record");

        assert_eq!(record.snapshot.phase(), ProductRunPhase::RecoveryRequired);
        assert_eq!(record.resume.as_ref().expect("resume").next_phase(), expected);
        assert_eq!(record.remaining_work, ["finish current phase"]);
        assert_eq!(record.interruption_cause, "daemon restart");
    }
}

fn durable_resume_json(identity: peritus_run_settlement::CandidateIdentity, phase: u16) -> Vec<u8> {
    use serde_json::Value;

    let missing = json_object([
        ("status", Value::from(1)),
        ("provenance", Value::Null),
        ("value", Value::Null),
    ]);
    let identity = json_object([
        ("run_id", serde_json::to_value(identity.run_id().as_bytes()).expect("run ID")),
        (
            "workspace_id",
            serde_json::to_value(identity.workspace_id().as_bytes()).expect("workspace ID"),
        ),
        (
            "content_digest",
            serde_json::to_value(identity.content_digest().as_bytes()).expect("content digest"),
        ),
        (
            "repository_digest",
            serde_json::to_value(identity.repository_digest().as_bytes())
                .expect("repository digest"),
        ),
        ("requirements_revision", Value::from(identity.requirements_revision())),
        ("checkpoint_sequence", Value::from(identity.checkpoint_sequence())),
    ]);
    let checkpoint = json_object([
        ("identity", identity),
        ("stage", Value::from(CandidateStage::Changed.tag())),
        ("gates", missing.clone()),
        ("obligations", missing.clone()),
        ("review", missing),
    ]);
    serde_json::to_vec(&json_object([
        ("version", Value::from(2)),
        ("checkpoint", checkpoint),
        ("baseline_head", Value::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
        ("next_phase", Value::from(phase)),
        ("design_path", Value::from(".design/tetris.md")),
        ("design_markdown", Value::from("# Tetris design")),
        ("design_revision", Value::from(1)),
        ("task_summary", Value::from("writer state retained")),
        ("run_instructions", Value::from("cargo run")),
        ("fix_summaries", Value::Array(Vec::new())),
        ("tool_calls", Value::from(4)),
        ("finding_state", Value::from("")),
        ("diff", Value::from("diff --git")),
        ("gates", Value::from("")),
        ("review", Value::from("")),
        ("developer_evidence", Value::from("writer completed")),
        ("successful_commands", Value::Array(Vec::new())),
        ("fixer_cycles", Value::from(0)),
    ]))
    .expect("durable JSON")
}

fn json_object<const N: usize>(entries: [(&str, serde_json::Value); N]) -> serde_json::Value {
    serde_json::Value::Object(
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect(),
    )
}
