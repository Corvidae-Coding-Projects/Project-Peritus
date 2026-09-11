//! Real workspace inspection progress and grounding remain separate host facts.

use super::*;

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
