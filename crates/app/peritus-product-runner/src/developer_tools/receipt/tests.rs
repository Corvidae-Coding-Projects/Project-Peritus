//! Effect-receipt replay, conflict, corruption, and capacity tests.

use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits, ToolCallId, ToolName};

use super::*;

#[test]
fn completed_effect_replays_and_conflicting_request_is_refused() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let original = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
    let mut output = serde_json::Map::new();
    output.insert("changed".to_owned(), Value::Bool(true));
    first.complete(&Value::Object(output), false).expect("complete");

    let mut replay = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(
        replay.begin(&original).expect("replay"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
    let reassigned = call("call-2", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let mut reassigned_replay = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(
        reassigned_replay.begin(&reassigned).expect("replay with new provider ID"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
    let conflicting = call("call-2", "workspace_write", r#"{"content":"two","path":"a"}"#);
    let mut conflict = EffectReceiptLedger::new(path, "writer-1".to_owned());
    assert!(matches!(
        conflict.begin(&conflicting).expect("conflict"),
        ReceiptDecision::Refuse { detail, ambiguous: false } if detail.contains("differs")
    ));
}

#[test]
fn interrupted_command_is_durably_ambiguous_and_never_relaunched() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let call = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&call).expect("start"), ReceiptDecision::Execute));

    let mut stable_length = None;
    for _ in 0..2 {
        let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(
            recovered.begin(&call).expect("recover"),
            ReceiptDecision::Refuse { detail, ambiguous: true }
                if detail.contains("ambiguous prior command outcome")
        ));
        let length = fs::metadata(&path).expect("ledger metadata").len();
        if let Some(expected) = stable_length {
            assert_eq!(length, expected, "persisted ambiguity must be restart-stable");
        } else {
            stable_length = Some(length);
        }
    }
}

#[test]
fn interrupted_non_command_effect_remains_executable() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&call).expect("start"), ReceiptDecision::Execute));

    let mut recovered = EffectReceiptLedger::new(path, "writer-1".to_owned());
    assert!(matches!(recovered.begin(&call).expect("recover"), ReceiptDecision::Execute));
}

#[test]
fn reused_provider_call_id_conflicts_on_each_identity_dimension() {
    for conflicting in [
        call("call-1", "workspace_delete", r#"{"path":"a"}"#),
        call("call-1", "workspace_write", r#"{"content":"two","path":"a"}"#),
    ] {
        let directory = tempfile::tempdir().expect("state");
        let path = directory.path().join("effects.bin");
        let original = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
        let mut ledger = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(ledger.begin(&original).expect("first"), ReceiptDecision::Execute));
        ledger.complete(&Value::Bool(true), false).expect("complete first");

        let mut recovered = EffectReceiptLedger::new(path, "writer-1".to_owned());
        assert!(matches!(
            recovered.begin(&original).expect("replay first"),
            ReceiptDecision::Replay { .. }
        ));
        assert!(matches!(
            recovered.begin(&conflicting).expect("reject reused call ID"),
            ReceiptDecision::Refuse { detail, ambiguous: false }
                if detail.contains("provider reused one tool-call ID")
        ));
    }
}

#[cfg(unix)]
#[test]
fn receipt_read_errors_are_not_treated_as_an_empty_ledger() {
    let directory = tempfile::tempdir().expect("state");
    let mut ledger =
        EffectReceiptLedger::new(directory.path().to_path_buf(), "writer-1".to_owned());
    let call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let Err(error) = ledger.begin(&call) else {
        panic!("reading a directory must fail closed");
    };
    assert!(error.to_string().contains("read effect receipts"));
}

#[test]
fn ledger_byte_bound_accepts_exact_limit_and_rejects_one_byte_over() {
    let directory = tempfile::tempdir().expect("state");
    let call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    for (length, expected) in [
        (MAX_LEDGER_BYTES as u64, "decode effect receipt"),
        (MAX_LEDGER_BYTES as u64 + 1, "effect receipt ledger exceeds its byte bound"),
    ] {
        let path = directory.path().join(format!("ledger-{length}.bin"));
        let file = fs::File::create(&path).expect("create bounded sparse ledger");
        file.set_len(length).expect("size bounded sparse ledger");
        drop(file);
        let mut ledger = EffectReceiptLedger::new(path, "writer-1".to_owned());
        let Err(error) = ledger.begin(&call) else {
            panic!("synthetic ledger must fail closed");
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn record_byte_bound_accepts_exact_limit_and_rejects_one_byte_over() {
    let directory = tempfile::tempdir().expect("state");
    let call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    for (length, expected) in [
        (MAX_RECORD_BYTES as u64, "decode effect receipt"),
        (MAX_RECORD_BYTES as u64 + 1, "effect receipt record exceeds its byte bound"),
    ] {
        let path = directory.path().join(format!("record-{length}.bin"));
        let mut file = fs::File::create(&path).expect("create bounded record ledger");
        file.write_all(&length.to_le_bytes()).expect("record length");
        file.set_len(length + 8).expect("size bounded sparse record");
        drop(file);
        let mut ledger = EffectReceiptLedger::new(path, "writer-1".to_owned());
        let Err(error) = ledger.begin(&call) else {
            panic!("synthetic record must fail closed");
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
#[ignore = "subprocess fixture; invoked by process_termination_preserves_receipt_decisions"]
fn receipt_process_child_fixture() {
    let Ok(mode) = std::env::var("PERITUS_RECEIPT_CRASH_MODE") else { return };
    let path = PathBuf::from(std::env::var("PERITUS_RECEIPT_CRASH_LEDGER").expect("ledger"));
    let effects = PathBuf::from(std::env::var("PERITUS_RECEIPT_CRASH_EFFECTS").expect("effects"));
    let barrier = PathBuf::from(std::env::var("PERITUS_RECEIPT_CRASH_BARRIER").expect("barrier"));
    let (tool_name, arguments) = match mode.as_str() {
        "started" => ("run_command", r#"{"args":[],"program":"example"}"#),
        "completed" => ("workspace_write", r#"{"content":"one","path":"a"}"#),
        _ => panic!("unknown receipt crash mode"),
    };
    let call = call("call-1", tool_name, arguments);
    let mut ledger = EffectReceiptLedger::new(path, "writer-1".to_owned());
    assert!(matches!(ledger.begin(&call).expect("start effect"), ReceiptDecision::Execute));
    let mut effect_file =
        OpenOptions::new().create(true).append(true).open(effects).expect("effect counter");
    effect_file.write_all(b"effect\n").expect("record effect");
    effect_file.sync_data().expect("persist effect counter");
    if mode == "completed" {
        ledger.complete(&Value::Bool(true), false).expect("complete receipt");
    }
    fs::write(barrier, format!("{mode}-reached\n")).expect("publish receipt barrier");
    loop {
        std::thread::park();
    }
}

#[test]
fn process_termination_preserves_receipt_decisions() {
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let executable = std::env::current_exe().expect("current test executable");
    for mode in ["started", "completed"] {
        let directory = tempfile::tempdir().expect("state");
        let ledger_path = directory.path().join("effects.bin");
        let effects_path = directory.path().join("effect-count.txt");
        let barrier_path = directory.path().join("receipt.barrier");
        let child = std::process::Command::new(&executable)
            .args([
                "--exact",
                "developer_tools::receipt::tests::receipt_process_child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PERITUS_RECEIPT_CRASH_MODE", mode)
            .env("PERITUS_RECEIPT_CRASH_LEDGER", &ledger_path)
            .env("PERITUS_RECEIPT_CRASH_EFFECTS", &effects_path)
            .env("PERITUS_RECEIPT_CRASH_BARRIER", &barrier_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn owned receipt fixture");
        let mut child = OwnedChild(child);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !barrier_path.exists() {
            assert!(std::time::Instant::now() < deadline, "receipt fixture timed out");
            assert!(child.0.try_wait().expect("inspect child").is_none(), "fixture exited early");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.0.kill().expect("terminate exact owned receipt fixture");
        assert!(!child.0.wait().expect("reap receipt fixture").success());

        let (tool_name, arguments) = if mode == "started" {
            ("run_command", r#"{"args":[],"program":"example"}"#)
        } else {
            ("workspace_write", r#"{"content":"one","path":"a"}"#)
        };
        let call = call("call-1", tool_name, arguments);
        let mut recovered = EffectReceiptLedger::new(ledger_path, "writer-1".to_owned());
        let decision = recovered.begin(&call).expect("recover receipt");
        if mode == "started" {
            assert!(matches!(decision, ReceiptDecision::Refuse { ambiguous: true, .. }));
        } else {
            assert!(matches!(decision, ReceiptDecision::Replay { is_error: false, .. }));
        }
        assert_eq!(fs::read_to_string(effects_path).expect("effect counter"), "effect\n");
    }
}

#[test]
fn append_after_truncated_tail_remains_replayable_after_another_restart() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let first_call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let second_call = call("call-2", "workspace_write", r#"{"content":"two","path":"b"}"#);
    let output = Value::Bool(true);

    let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(first.begin(&first_call).expect("start first"), ReceiptDecision::Execute));
    first.complete(&output, false).expect("complete first");
    let clean_length = fs::metadata(&path).expect("clean ledger metadata").len();
    let mut interrupted = OpenOptions::new().append(true).open(&path).expect("open ledger");
    interrupted.write_all(&64_u64.to_le_bytes()).expect("partial frame length");
    interrupted.write_all(b"{").expect("partial frame payload");
    interrupted.sync_data().expect("persist simulated crash tail");
    drop(interrupted);

    let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    assert!(matches!(
        recovered.begin(&first_call).expect("replay first"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
    assert_eq!(
        fs::metadata(&path).expect("recovered ledger metadata").len(),
        clean_length,
        "recovery removes only the incomplete tail before accepting another effect",
    );
    assert!(matches!(
        recovered.begin(&second_call).expect("start second"),
        ReceiptDecision::Execute
    ));
    recovered.complete(&output, false).expect("complete second");

    let mut restarted = EffectReceiptLedger::new(path, "writer-1".to_owned());
    assert!(matches!(
        restarted.begin(&first_call).expect("replay first after restart"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
    assert!(matches!(
        restarted.begin(&second_call).expect("replay second after restart"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
}

#[test]
fn complete_corrupt_frame_fails_closed_without_truncation() {
    let directory = tempfile::tempdir().expect("state");
    let path = directory.path().join("effects.bin");
    let payload = b"not-json";
    let mut file = OpenOptions::new().create(true).append(true).open(&path).expect("ledger");
    let length = u64::try_from(payload.len()).expect("payload length");
    file.write_all(&length.to_le_bytes()).expect("frame length");
    file.write_all(payload).expect("complete corrupt payload");
    file.sync_data().expect("persist corrupt frame");
    let corrupt_length = fs::metadata(&path).expect("corrupt ledger metadata").len();

    let call = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
    let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
    let Err(error) = recovered.begin(&call) else {
        panic!("complete corrupt frame must fail closed");
    };
    assert!(error.to_string().contains("decode effect receipt"));
    assert_eq!(fs::metadata(path).expect("retained corrupt ledger").len(), corrupt_length);
}

fn call(id: &str, name: &str, arguments: &str) -> CompletedToolCall {
    CompletedToolCall::new(
        ToolCallId::new(id.to_owned()).expect("call ID"),
        ToolName::new(name.to_owned()).expect("tool name"),
        CanonicalJson::parse(arguments, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("arguments"),
    )
    .expect("completed call")
}
