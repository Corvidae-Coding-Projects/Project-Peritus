use super::*;

#[test]
fn canonical_record_rejects_omitted_current_fields() {
    let record = PersistedRecord {
        format_version: 3,
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

    let mut missing_qualification = canonical.clone();
    missing_qualification["deliverable"]
        .as_object_mut()
        .expect("deliverable object")
        .remove("qualification");
    assert!(
        serde_json::from_value::<PersistedRecord>(missing_qualification).is_err(),
        "omitted qualification must not become an implicitly qualified candidate",
    );

    let mut previous = canonical;
    previous["format_version"] = serde_json::Value::from(2);
    let previous = serde_json::from_value::<PersistedRecord>(previous)
        .expect("complete previous-format shape");
    assert!(
        previous.into_record().is_err(),
        "a complete previous-format record must not enter current state",
    );
}
