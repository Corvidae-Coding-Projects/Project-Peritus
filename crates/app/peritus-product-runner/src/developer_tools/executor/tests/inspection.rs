//! Real workspace inspection progress and grounding remain separate host facts.

use super::*;
use peritus_model_protocol::{ContentBlock, Message, Role, ToolResult};

fn inspection_view(call: &CompletedToolCall, result: &DeveloperToolObservation) -> Vec<Message> {
    let mut output: Value = serde_json::from_slice(result.output.canonical_bytes()).unwrap();
    output.as_object_mut().unwrap().insert(
        "local_context".to_owned(),
        serde_json::json!({"handle":"obs:test:000001","authority":"none"}),
    );
    let output =
        CanonicalJson::parse(&output.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
            .unwrap();
    vec![
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolCall(call.clone())],
            ProtocolLimits::PRODUCTION,
        )
        .unwrap(),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(call.id().clone(), output, false))],
            ProtocolLimits::PRODUCTION,
        )
        .unwrap(),
    ]
}

#[test]
fn evicted_inspections_can_be_refetched_but_visible_cycles_still_stop() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "grounding\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_read", r#"{"path":"README.md"}"#);
    let mut last_call = completed_call("initial-list", "workspace_list", r#"{"path":"."}"#);
    let mut last_result = tools.execute(&last_call).unwrap();
    for index in 0..7 {
        tools.observe_model_context(&[]).unwrap();
        last_call =
            completed_call(&format!("refetch-{index}"), "workspace_list", r#"{"path":"."}"#);
        last_result = tools.execute(&last_call).unwrap();
        assert!(!last_result.is_error);
        assert!(tools.required_tool_name().is_none());
        assert!(tools.continuation_blocker().is_none(), "evicted evidence can be refetched");
        assert!(tools.take_progress_feedback().is_none());
    }
    for repeat in 1..=6 {
        tools.observe_model_context(&inspection_view(&last_call, &last_result)).unwrap();
        last_call =
            completed_call(&format!("visible-{repeat}"), "workspace_list", r#"{"path":"."}"#);
        last_result = tools.execute(&last_call).unwrap();
        assert_eq!(tools.continuation_blocker().is_some(), repeat == 6);
        assert_eq!(
            tools
                .take_progress_feedback()
                .is_some_and(|feedback| feedback.contains("inspection-no-progress")),
            repeat == 3
        );
        assert!(tools.required_tool_name().is_none());
    }
}

#[test]
fn reused_call_ids_do_not_make_evicted_inspections_visible() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("first.txt"), "first\n").unwrap();
    fs::write(workspace.path().join("second.txt"), "second\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    let first = completed_call("reused-id", "workspace_read", r#"{"path":"first.txt"}"#);
    tools.execute(&first).unwrap();
    let other = completed_call("reused-id", "workspace_read", r#"{"path":"second.txt"}"#);
    let other_result = tools.execute(&other).unwrap();
    for index in 0..7 {
        tools.observe_model_context(&inspection_view(&other, &other_result)).unwrap();
        let refetch = completed_call(
            &format!("refetch-{index}"),
            "workspace_read",
            r#"{"path":"first.txt"}"#,
        );
        assert!(!tools.execute(&refetch).unwrap().is_error);
        assert!(tools.continuation_blocker().is_none());
        assert!(tools.take_progress_feedback().is_none());
    }
}

#[test]
fn batched_repeated_listings_warn_before_stopping_and_preserve_grounding() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "grounding\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    execute(&mut tools, "workspace_read", r#"{"path":"README.md"}"#);
    for _ in 0..6 {
        assert!(!execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#).is_error);
    }
    assert!(tools.required_tool_name().is_none());
    assert!(tools.continuation_blocker().is_none());
    assert!(tools.take_progress_feedback().unwrap().contains("inspection-no-progress"));
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    assert!(tools.continuation_blocker().is_some());
    assert!(tools.required_tool_name().is_none());
}

#[test]
fn repeated_successful_listings_warn_and_block_without_resetting_grounding() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "grounding\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    execute(&mut tools, "workspace_read", r#"{"path":"README.md"}"#);
    assert!(tools.required_tool_name().is_none());
    for repeat in 1..=6 {
        let result = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
        assert!(!result.is_error);
        assert!(tools.required_tool_name().is_none());
        assert_eq!(tools.continuation_blocker().is_some(), repeat == 6);
        let warning = tools.take_progress_feedback();
        assert_eq!(warning.is_some(), repeat == 3);
        if let Some(warning) = warning {
            assert!(warning.contains("inspection-no-progress"));
        }
    }
    assert!(tools.continuation_blocker().unwrap().contains("6 consecutive"));
    fs::write(workspace.path().join("new.txt"), "new evidence").unwrap();
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    assert!(tools.continuation_blocker().is_none());
}

#[test]
fn read_only_inspection_and_real_delivery_do_not_exhaust_the_nudge_budget() {
    let workspace = tempfile::tempdir().unwrap();
    let mut readonly = WorkspaceDeveloperTools::read_only(workspace.path().to_path_buf());
    for _ in 0..20 {
        execute(&mut readonly, "workspace_list", r#"{"depth":1,"path":"."}"#);
    }
    assert!(readonly.continuation_blocker().is_none());
    assert!(readonly.take_progress_feedback().is_none());

    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    tools.progress_nudges = 2;
    execute(&mut tools, "workspace_write", r#"{"path":"new.txt","content":"new"}"#);
    assert_eq!(tools.progress_nudges, 0);
    execute(&mut tools, "workspace_read", r#"{"path":"new.txt"}"#);
    tools.progress_nudges = 2;
    let unchanged = execute(&mut tools, "workspace_write", r#"{"path":"new.txt","content":"new"}"#);
    assert!(!unchanged.is_error);
    assert_eq!(tools.progress_nudges, 2, "an unchanged write is not delivery progress");
}
