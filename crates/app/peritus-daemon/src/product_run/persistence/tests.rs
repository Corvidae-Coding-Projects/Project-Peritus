use super::*;

#[test]
fn legacy_run_without_messages_gains_a_resumable_conversation() {
    let json = r#"{
        "run_id":"01010101010101010101010101010101",
        "workspace_id":"02020202020202020202020202020202",
        "writer":"03030303030303030303030303030303",
        "reviewer":"04040404040404040404040404040404",
        "fixer":"05050505050505050505050505050505",
        "phase":8,
        "cycle":1,
        "task":"build tetris",
        "status":"parse model file plan failed",
        "diff":"",
        "gates":"",
        "review":"",
        "summary":"invalid escape"
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("legacy record");
    let record = persisted.into_record().expect("migrated record");
    let conversation = record.conversation.snapshot().expect("conversation");

    assert_eq!(record.snapshot.phase(), ProductRunPhase::Failed);
    assert_eq!(record.progress.provider_failovers, 0);
    assert_eq!(record.progress.workspace_growth_bytes, 0);
    assert_eq!(record.progress.peak_rss_bytes, 0);
    assert_eq!(conversation.messages().len(), 2);
    assert_eq!(conversation.messages()[0].content(), "build tetris");
    assert!(conversation.messages()[1].content().contains("invalid escape"));
}

#[test]
fn durable_finding_state_survives_record_restoration() {
    let json = r#"{
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
        "finding_state":"{\"cycle\":1,\"summary\":\"nested target finding\",\"findings\":[]}"
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("persisted record");
    let expected = persisted.finding_state.clone();

    let record = persisted.into_record().expect("restored record");

    assert_eq!(record.finding_state, expected);
}

#[test]
fn candidate_qualification_is_independent_from_user_disposition() {
    let json = r#"{
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
        }
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("candidate record");
    let record = persisted.into_record().expect("restored record");
    let deliverable = record.snapshot.deliverable().expect("deliverable");

    assert_eq!(deliverable.qualification(), CandidateStage::Changed);
    assert!(deliverable.accepted());
    assert_eq!(deliverable.export_path(), "/tmp/tetris.patch");
}

#[test]
fn legacy_deliverable_without_qualification_migrates_as_qualified() {
    let json = r#"{
        "run_id":"31313131313131313131313131313131",
        "workspace_id":"32323232323232323232323232323232",
        "writer":"33333333333333333333333333333333",
        "reviewer":"34343434343434343434343434343434",
        "fixer":"35353535353535353535353535353535",
        "phase":7,
        "cycle":1,
        "task":"build tetris",
        "status":"complete",
        "diff":"diff --git",
        "gates":"pass",
        "review":"pass",
        "summary":"done",
        "deliverable":{
            "workspace_path":"/managed/tetris",
            "changed_paths":["src/main.rs"],
            "successful_commands":["cargo test"],
            "run_instructions":"cargo run",
            "accepted":false,
            "commit_revision":"",
            "export_path":"",
            "discarded":false
        }
    }"#;
    let persisted: PersistedRecord = serde_json::from_str(json).expect("legacy record");
    let record = persisted.into_record().expect("migrated record");

    assert_eq!(
        record.snapshot.deliverable().expect("deliverable").qualification(),
        CandidateStage::Qualified,
    );
}

#[test]
fn restart_restores_each_resumable_phase_and_preserves_completed_writer_state() {
    use peritus_run_settlement::{CandidateCheckpoint, CandidateIdentity, EvidenceStatus};
    use peritus_types::Sha256Digest;

    let run_id = RunId::new([0x41; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0x42; 16]).expect("workspace");
    let identity =
        CandidateIdentity::new(run_id, workspace_id, Sha256Digest::new([0x43; 32]), 1, 2)
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
            finding_state: String::new(),
            deliverable: Some(PersistedDeliverable {
                workspace_path: "/managed/tetris".to_owned(),
                changed_paths: vec!["src/main.rs".to_owned()],
                successful_commands: Vec::new(),
                run_instructions: "cargo run".to_owned(),
                qualification: Some(CandidateStage::Changed.tag()),
                accepted: false,
                commit_revision: String::new(),
                export_path: String::new(),
                discarded: false,
            }),
            messages: vec![PersistedMessage {
                role: ProductConversationRole::User.tag(),
                content: "build tetris".to_owned(),
            }],
            progress: PersistedProgress::default(),
            checkpoint: Some(PersistedCheckpoint::from_checkpoint(&checkpoint)),
            settlement_cause: None,
            resume_state: Some(resume),
            remaining_work: vec!["finish current phase".to_owned()],
            interruption_cause: "daemon restart".to_owned(),
            candidate_actionable: Some(true),
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
            "candidate_digest",
            serde_json::to_value(identity.candidate_digest().as_bytes()).expect("digest"),
        ),
        ("conversation_revision", Value::from(identity.conversation_revision())),
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
        ("version", Value::from(1)),
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
