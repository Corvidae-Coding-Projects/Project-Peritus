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
