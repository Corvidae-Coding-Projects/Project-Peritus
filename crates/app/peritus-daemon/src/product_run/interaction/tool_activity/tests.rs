use super::*;
use peritus_app_protocol::{MAX_PRODUCT_ACTIVITY_BYTES, ProductInteractionMode, ProductRoleModels};

fn options() -> InteractionOptions {
    InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default())
}

#[test]
fn command_is_visible_before_completion_and_same_entry_gains_real_output() {
    let mut options = options();
    started(&mut options, "run_command", r#"{"program":"cargo","args":["test","a b"],"cwd":"."}"#)
        .unwrap();
    assert_eq!(options.activities.len(), 1);
    assert_eq!(options.activities[0].text(), "Calling cargo test 'a b' (cwd .)");
    assert_eq!(options.activities[0].detail(), "Awaiting tool result");
    options.append(ProductActivityKind::User, "Please continue", "").unwrap();
    finished(
        &mut options,
        "run_command",
        r#"{"exit_code":0,"stdout":"2 passed\n","stderr":"","state":"completed"}"#,
        false,
    )
    .unwrap();
    assert_eq!(options.activities.len(), 2);
    assert_eq!(options.activities[0].sequence(), 1);
    assert_eq!(options.activities[0].text(), "Ran cargo test 'a b' (cwd .)");
    assert!(options.activities[0].detail().contains("Exit code: 0"));
    assert!(options.activities[0].detail().contains("2 passed"));
    assert_eq!(options.activities[1].kind(), ProductActivityKind::User);
}

#[test]
fn rejected_commands_and_background_handles_do_not_claim_completed_success() {
    let mut options = options();
    started(&mut options, "run_command", r#"{"program":"cargo","args":["test"]}"#).unwrap();
    finished(&mut options, "run_command", r#"{"error":"No action was executed"}"#, true).unwrap();
    assert!(options.activities[0].text().starts_with("Failed cargo test"));
    assert!(options.activities[0].detail().contains("No action was executed"));
    started(&mut options, "command_start", r#"{"program":"cargo","args":["test"]}"#).unwrap();
    finished(
        &mut options,
        "command_start",
        r#"{"state":"running","handle":"owned-1","success":true}"#,
        false,
    )
    .unwrap();
    assert!(options.activities[1].text().starts_with("Started cargo test"));
    assert!(options.activities[1].detail().contains("owned-1"));
    started(&mut options, "workspace_read", r#"{"path":"file.rs"}"#).unwrap();
    finished(&mut options, "command_poll", r#"{"state":"running"}"#, false).unwrap();
    assert!(options.activities[2].text().starts_with("Calling workspace_read"));
    assert!(options.activities[3].text().starts_with("Started command_poll"));
}

#[test]
fn previews_bound_unicode_and_omit_bodies_and_recognizable_credentials() {
    let summary =
        display::summary("workspace_write", r#"{"path":"file.rs","content":"private body"}"#);
    assert!(summary.contains("file.rs"));
    assert!(!summary.contains("private body"));
    for arguments in [
        r#"{"program":"client","args":["--token","secret-canary"]}"#,
        r#"{"program":"client","args":["--password=secret-canary"]}"#,
    ] {
        assert!(!display::summary("run_command", arguments).contains("secret-canary"));
    }
    let output = serde_json::json!({"stdout":"Authorization: Bearer secret-canary\nordinary output", "stderr":"", "exit_code":1}).to_string();
    let preview = display::result("run_command", &output);
    assert!(!preview.contains("secret-canary"));
    assert!(preview.contains("ordinary output"));
    let output =
        serde_json::json!({"stdout":"é".repeat(20_000), "stderr":"final failure"}).to_string();
    let preview = display::result("run_command", &output);
    assert!(preview.len() <= MAX_PRODUCT_ACTIVITY_BYTES);
    assert!(preview.contains("preview truncated"));
    assert!(preview.ends_with("final failure"));
}
