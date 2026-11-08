//! Real file-range and active-work selection regressions.
use crate::local_context::praxis_support::fixture::{Fixture, profile};
use peritus_model_protocol::{ContentBlock, Message};
use peritus_role::HarnessRole;
use serde_json::json;
use std::{fmt::Write as _, fs};

#[test]
fn overlapping_inspection_arguments_do_not_multiply_the_same_current_file_body() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    let mut contents = String::new();
    for n in 1..=100 {
        writeln!(contents, "BODY_LINE_{n:03} {}", "detail ".repeat(10)).unwrap();
    }
    fs::write(f.workspace.join("file.txt"), contents).unwrap();
    for start in 1..=24 {
        let (_, failed, _) = f
            .execute(
                "workspace_read",
                &json!({"path":"file.txt","start_line":start,"end_line":100,"max_bytes":32768}),
                false,
            )
            .unwrap();
        assert!(!failed);
    }
    f.reopen().unwrap();
    let mut memory = f.memory().unwrap().lock().unwrap();
    let messages = memory
        .prepare_view_with_governing(&profile(828_144, 256).unwrap(), &[], None, None, None, None)
        .unwrap();
    let cards = messages
        .iter()
        .flat_map(Message::content)
        .filter_map(
            |b| if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None },
        )
        .filter(|t| t.starts_with("DECISION TOOL EVIDENCE V1") && t.contains("tool=workspace_read"))
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 1, "one broad original card plus the current native exchange suffices");
    assert!(cards[0].contains("BODY_LINE_001"));
    assert!(cards[0].contains("BODY_LINE_100"));
    assert!(peritus_agent::estimate_developer_request_tokens(&messages, &[]) < 30_000);
    drop(memory);
}

#[test]
fn unchanged_reference_material_is_not_all_promoted_into_the_active_implementation() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    for n in 0..24 {
        let path = format!("reference-{n}.txt");
        fs::write(
            f.workspace.join(&path),
            format!("REFERENCE_BODY_{n} {}", "reference ".repeat(2000)),
        )
        .unwrap();
        let (_, failed, _) =
            f.execute("workspace_read", &json!({"path":path,"max_bytes":32768}), false).unwrap();
        assert!(!failed);
    }
    // Replay a settled host write against independently checked real file state.
    fs::write(f.workspace.join("work.txt"), "ACTIVE_IMPLEMENTATION_BODY").unwrap();
    super::praxis_recovery::archive(
        &f,
        "workspace_write",
        &json!({"path":"work.txt","changed":true}),
        false,
    );
    f.execute("workspace_read", &json!({"path":"work.txt","max_bytes":4096}), false).unwrap();
    let scope = crate::local_context::tools::hex(
        f.memory().unwrap().lock().unwrap().store.scope_digest().as_bytes(),
    );
    f.execute("context_sources", &json!({"scope":scope,"cursor":null,"max_bytes":1024}), true)
        .unwrap();
    f.reopen().unwrap();
    let mut memory = f.memory().unwrap().lock().unwrap();
    let messages = memory
        .prepare_view_with_governing(&profile(828_144, 256).unwrap(), &[], None, None, None, None)
        .unwrap();
    let rendered = messages
        .iter()
        .flat_map(Message::content)
        .filter_map(
            |b| if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None },
        )
        .collect::<String>();
    assert!(rendered.contains("ACTIVE_IMPLEMENTATION_BODY"));
    assert!(
        peritus_agent::estimate_developer_request_tokens(&messages, &[]) < 100_000,
        "spare capacity must not refill the active task with unrelated references"
    );
    drop(memory);
}

#[test]
fn a_native_tool_with_similar_json_fields_does_not_substitute_for_a_workspace_read() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    fs::write(f.workspace.join("file.txt"), "REAL_FILE_BODY").unwrap();
    let (mut lookalike, failed, _) =
        f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), false).unwrap();
    assert!(!failed);
    lookalike["content"] = json!("UNTRUSTED_LOOKALIKE");
    super::praxis_recovery::archive(&f, "module_read_blob", &lookalike, false);
    let mut memory = f.memory().unwrap().lock().unwrap();
    let messages = memory
        .prepare_view_with_governing(&profile(828_144, 256).unwrap(), &[], None, None, None, None)
        .unwrap();
    assert!(messages.iter().flat_map(Message::content).any(|b|matches!(b,ContentBlock::Text(t) if t.expose_for_wire().starts_with("DECISION TOOL EVIDENCE V1") && t.expose_for_wire().contains("tool=workspace_read") && t.expose_for_wire().contains("REAL_FILE_BODY"))));
    drop(memory);
}
