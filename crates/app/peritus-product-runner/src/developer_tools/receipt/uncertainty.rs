//! Unknown command inspection, acknowledgement, and cross-invocation fencing.

use std::{fs, io::Write as _};

use super::*;

#[test]
fn uncertain_command_inspection_is_read_only_and_identity_stable_across_recovery() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let call = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&call).expect("start"), ReceiptDecision::Execute));
    let started_bytes = fs::read(&path).expect("started ledger");

    let started = uncertain_effects(&path).expect("inspect started command");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].tool(), "run_command");
    assert_eq!(started[0].state(), UncertainEffectState::Started);
    assert_eq!(fs::read(&path).expect("ledger after inspection"), started_bytes);

    let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(
        recovered.begin(&call).expect("recover"),
        ReceiptDecision::Refuse { ambiguous: true, .. }
    ));
    let ambiguous = uncertain_effects(&path).expect("inspect ambiguous command");
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].identity(), started[0].identity());
    assert_eq!(ambiguous[0].state(), UncertainEffectState::Ambiguous);
}

#[test]
fn acknowledged_uncertain_command_replays_unknown_error_without_relaunch() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let call = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&call).expect("start"), ReceiptDecision::Execute));
    let effect = uncertain_effects(&path).expect("inspect").pop().expect("uncertain command");

    acknowledge_uncertain_effect(&path, effect.identity()).expect("acknowledge uncertainty");
    let reviewed = uncertain_effects(&path).expect("inspect reviewed");
    assert_eq!(reviewed.len(), 1);
    assert_eq!(reviewed[0].state(), UncertainEffectState::Reviewed);
    let retained_length = fs::metadata(&path).expect("reviewed ledger").len();

    for _ in 0..2 {
        let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(
            recovered.begin(&call).expect("replay reviewed outcome"),
            ReceiptDecision::Replay { value, is_error: true }
                if value["reviewed"] == true && value["outcome_unknown"] == true
        ));
        assert_eq!(
            fs::metadata(&path).expect("replayed ledger").len(),
            retained_length,
            "replaying a reviewed unknown outcome must not append or relaunch",
        );
    }
}

#[test]
fn acknowledged_command_is_blocked_across_fresh_invocation_scopes() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-old".to_owned(),
    );
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    let effect = uncertain_effects(&path).expect("inspect").pop().expect("uncertain command");
    acknowledge_uncertain_effect(&path, effect.identity()).expect("acknowledge uncertainty");
    let retained_length = fs::metadata(&path).expect("reviewed ledger").len();

    let reassigned = call("call-2", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut resumed = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-fixer-7-revision-2-invocation-4-new".to_owned(),
    );
    assert!(matches!(
        resumed.replay(&reassigned).expect("cross-scope replay"),
        Some(ReceiptDecision::Replay { value, is_error: true })
            if value["reviewed"] == true && value["outcome_unknown"] == true
    ));
    assert_eq!(
        fs::metadata(path).expect("replayed ledger").len(),
        retained_length,
        "a fresh invocation must observe the old command barrier without appending",
    );
}

#[test]
fn reviewed_unknown_command_freezes_other_mutations_in_the_same_requirements_revision() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-old".to_owned(),
    );
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    let effect = uncertain_effects(&path).expect("inspect").pop().expect("uncertain command");
    acknowledge_uncertain_effect(&path, effect.identity()).expect("acknowledge uncertainty");

    let replacement =
        call("call-2", "workspace_remove", r#"{"path":"command-recovery-marker.txt"}"#);
    let mut fixer = EffectReceiptLedger::new(
        path,
        "peritus-11111111111111111111111111111111-fixer-7-revision-2-invocation-4-new".to_owned(),
    );
    assert!(matches!(
        fixer.replay(&replacement).expect("cross-role mutation fence"),
        Some(ReceiptDecision::Refuse { detail, ambiguous: true })
            if detail.contains("new mutating effects are blocked")
    ));
}

#[test]
fn unresolved_command_is_refused_across_fresh_invocation_scopes() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-old".to_owned(),
    );
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    let retained_length = fs::metadata(&path).expect("started ledger").len();

    let reassigned = call("call-2", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut resumed = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-fixer-7-revision-2-invocation-4-new".to_owned(),
    );
    assert!(matches!(
        resumed.replay(&reassigned).expect("cross-scope refusal"),
        Some(ReceiptDecision::Refuse { ambiguous: true, .. })
    ));
    assert_eq!(
        fs::metadata(path).expect("refused ledger").len(),
        retained_length,
        "refusing the old command under a fresh scope must remain read-only",
    );
}

#[test]
fn completed_command_does_not_block_a_later_invocation_scope() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-old".to_owned(),
    );
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    first.complete(&Value::Bool(true), false).expect("complete");

    let reassigned = call("call-2", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut later = EffectReceiptLedger::new(
        path,
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-new".to_owned(),
    );
    assert!(later.replay(&reassigned).expect("cross-scope completed lookup").is_none());
    assert!(matches!(later.begin(&reassigned).expect("new command"), ReceiptDecision::Execute));
}

#[test]
fn reviewed_command_does_not_block_a_new_requirements_revision() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(
        path.clone(),
        "peritus-11111111111111111111111111111111-writer-1-revision-2-invocation-1-old".to_owned(),
    );
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    let effect = uncertain_effects(&path).expect("inspect").pop().expect("uncertain command");
    acknowledge_uncertain_effect(&path, effect.identity()).expect("acknowledge uncertainty");

    let reassigned = call("call-2", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut next_task = EffectReceiptLedger::new(
        path,
        "peritus-11111111111111111111111111111111-writer-1-revision-3-invocation-1-new".to_owned(),
    );
    assert!(next_task.replay(&reassigned).expect("new requirements lookup").is_none());
    assert!(matches!(
        next_task.begin(&reassigned).expect("newly authorized command"),
        ReceiptDecision::Execute
    ));
}

#[test]
fn uncertain_inspection_excludes_completed_and_non_command_effects() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let mut ledger = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    let completed = call("call-1", "run_command", r#"{"args":[],"program":"done"}"#);
    assert!(matches!(ledger.begin(&completed).expect("start command"), ReceiptDecision::Execute));
    ledger.complete(&Value::Bool(true), false).expect("complete command");
    let non_command = call("call-2", "workspace_write", r#"{"content":"one","path":"a"}"#);
    assert!(matches!(ledger.begin(&non_command).expect("start write"), ReceiptDecision::Execute));

    assert!(uncertain_effects(&path).expect("inspect settled ledger").is_empty());
}

#[test]
fn uncertain_inspection_ignores_incomplete_tail_without_mutating_it() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let mut file = fs::File::create(&path).expect("ledger");
    file.write_all(&64_u64.to_le_bytes()).expect("partial length");
    file.write_all(b"{").expect("partial payload");
    file.sync_data().expect("persist partial frame");
    let bytes = fs::read(&path).expect("partial ledger");

    assert!(uncertain_effects(&path).expect("inspect committed frames").is_empty());
    assert_eq!(fs::read(&path).expect("retained partial ledger"), bytes);
}
