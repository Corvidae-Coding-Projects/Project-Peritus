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
fn evicted_inspections_can_be_refetched_and_visible_cycles_remain_advisory() {
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
    for repeat in 1..=32 {
        tools.observe_model_context(&inspection_view(&last_call, &last_result)).unwrap();
        last_call =
            completed_call(&format!("visible-{repeat}"), "workspace_list", r#"{"path":"."}"#);
        last_result = tools.execute(&last_call).unwrap();
        assert!(tools.continuation_blocker().is_none());
        assert_eq!(
            tools
                .take_progress_feedback()
                .is_some_and(|feedback| feedback.contains("repeated identical workspace")),
            repeat == 1
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
fn batched_repeated_listings_warn_and_preserve_grounding_without_stopping() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "grounding\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    execute(&mut tools, "workspace_read", r#"{"path":"README.md"}"#);
    for _ in 0..32 {
        assert!(!execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#).is_error);
    }
    assert!(tools.required_tool_name().is_none());
    assert!(tools.continuation_blocker().is_none());
    assert!(tools.take_progress_feedback().unwrap().contains("repeated identical workspace"));
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    assert!(tools.continuation_blocker().is_none());
    assert!(tools.required_tool_name().is_none());
}

#[test]
fn repeated_successful_listings_warn_without_blocking_or_resetting_grounding() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "grounding\n").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    execute(&mut tools, "workspace_read", r#"{"path":"README.md"}"#);
    assert!(tools.required_tool_name().is_none());
    for _ in 1..=32 {
        let result = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
        assert!(!result.is_error);
        assert!(tools.required_tool_name().is_none());
        assert!(tools.continuation_blocker().is_none());
        let warning = tools.take_progress_feedback();
        if let Some(warning) = warning {
            assert!(
                warning.contains("repeated identical workspace")
                    || warning.contains("long inspection sequence")
            );
        }
    }
    fs::write(workspace.path().join("new.txt"), "new evidence").unwrap();
    execute(&mut tools, "workspace_list", r#"{"depth":1,"path":"."}"#);
    assert!(tools.continuation_blocker().is_none());
}

#[test]
fn read_only_inspection_remains_advisory_and_real_delivery_resets_nudges() {
    let workspace = tempfile::tempdir().unwrap();
    let mut readonly = WorkspaceDeveloperTools::read_only(workspace.path().to_path_buf());
    for _ in 0..32 {
        execute(&mut readonly, "workspace_list", r#"{"depth":1,"path":"."}"#);
        let _ = readonly.take_progress_feedback();
    }
    assert!(readonly.continuation_blocker().is_none());

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

#[test]
fn visible_prior_observation_bounds_a_repeat_in_a_fresh_executor() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("README.md"), "stable\n").unwrap();
    let mut first = writable_tools(workspace.path());
    let call = completed_call("prior", "workspace_read", r#"{"path":"README.md"}"#);
    let result = first.execute(&call).unwrap();

    let mut resumed = writable_tools(workspace.path());
    resumed.observe_model_context(&inspection_view(&call, &result)).unwrap();
    let repeated = completed_call("repeated", "workspace_read", r#"{"path":"README.md"}"#);
    assert!(!resumed.execute(&repeated).unwrap().is_error);
    assert!(
        resumed
            .take_progress_feedback()
            .is_some_and(|feedback| feedback.contains("repeated identical workspace"))
    );
}

#[test]
fn large_text_files_support_bounded_line_ranges_and_continuations() {
    let workspace = tempfile::tempdir().unwrap();
    let mut content = "padding\n".repeat(300_000);
    content.push_str("TARGET-LINE\n");
    fs::write(workspace.path().join("large.txt"), content).unwrap();
    let mut tools = writable_tools(workspace.path());

    execute(&mut tools, "workspace_list", r#"{"path":".","depth":1}"#);
    let whole = execute(&mut tools, "workspace_read", r#"{"path":"large.txt"}"#);
    let ranged = execute(
        &mut tools,
        "workspace_read",
        r#"{"path":"large.txt","start_line":300001,"end_line":300002}"#,
    );

    assert!(!whole.is_error, "{}", wire(&whole));
    assert!(!ranged.is_error);
    let whole_page: Value = serde_json::from_slice(whole.output.canonical_bytes()).unwrap();
    let next_line = whole_page["next_line"].as_u64().expect("default window continuation");
    let next_offset = whole_page["next_line_byte_offset"].as_u64().expect("line offset");
    assert_eq!(next_line, 501);
    assert_eq!(next_offset, 0);
    let continuation = execute(
        &mut tools,
        "workspace_read",
        &serde_json::json!({
            "path":"large.txt",
            "start_line":next_line,
            "line_byte_offset":next_offset,
        })
        .to_string(),
    );
    assert!(!continuation.is_error, "{}", wire(&continuation));
    let continuation_page: Value =
        serde_json::from_slice(continuation.output.canonical_bytes()).unwrap();
    assert_eq!(continuation_page["start_line"], next_line);
    assert!(continuation_page["content"].as_str().unwrap().starts_with("501: padding"));
    assert!(continuation_page["next_line"].as_u64().unwrap() > next_line);
    let value: Value = serde_json::from_slice(ranged.output.canonical_bytes()).unwrap();
    assert_eq!(value["content"], "300001: TARGET-LINE");
}

#[test]
fn line_ranges_include_both_endpoints_and_unterminated_last_lines() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("lines.txt"), "first\nsecond\nlast").unwrap();
    let mut tools = writable_tools(workspace.path());
    execute(&mut tools, "workspace_list", r#"{"path":"."}"#);
    for (start, end, expected) in
        [(1, 1, "1: first"), (1, 2, "1: first\n2: second"), (3, 3, "3: last"), (4, 4, "")]
    {
        let result = execute(
            &mut tools,
            "workspace_read",
            &format!(r#"{{"path":"lines.txt","start_line":{start},"end_line":{end}}}"#),
        );
        assert!(!result.is_error);
        let value: Value = serde_json::from_slice(result.output.canonical_bytes()).unwrap();
        assert_eq!(value["content"], expected);
    }
}

#[cfg(unix)]
#[test]
fn listing_reports_dangling_symlinks_without_aborting_the_directory() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("healthy.txt"), "healthy").unwrap();
    symlink("missing.txt", workspace.path().join("dangling")).unwrap();
    let mut tools = writable_tools(workspace.path());

    let result = execute(&mut tools, "workspace_list", r#"{"path":".","depth":1}"#);

    assert!(!result.is_error);
    let value: Value = serde_json::from_slice(result.output.canonical_bytes()).unwrap();
    assert!(
        value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["path"] == "dangling" && entry["kind"] == "symlink" })
    );
    assert!(
        value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["path"] == "healthy.txt" && entry["kind"] == "file" })
    );
}
