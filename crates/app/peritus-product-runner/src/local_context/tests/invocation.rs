//! Current lifecycle projection is budgeted, retained, and replaced independently of history.

use super::support::*;
use peritus_model_protocol::{ContentBlock, Message, Role};

fn contains_completed_read(view: &[Message]) -> bool {
    view.iter().flat_map(Message::content).any(|block| {
        matches!(block, ContentBlock::ToolResult(result)
            if result.output().to_wire_string().contains("terminal modules already read"))
    })
}

#[test]
fn current_policy_replaces_checkpoint_state_without_erasing_completed_tools() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "recovery");
    observation(&mut memory, "read", "terminal modules already read", false);
    let first = message(Role::System, "Immutable policy\nprovider_step=2; required_tool=none");
    let profile = profile(32_768);
    let view = memory.prepare_view_with_policy(&profile, &[], Some(&first)).unwrap();
    assert_eq!(view.first(), Some(&first));
    assert!(contains_completed_read(&view));
    memory.publish(&view).unwrap();
    drop(memory);

    let mut memory = fixture.open();
    let next = message(Role::System, "Immutable policy\nprovider_step=3; required_tool=none");
    let view = memory.prepare_view_with_policy(&profile, &[], Some(&next)).unwrap();
    assert_eq!(view.first(), Some(&next));
    assert!(!render(&view).contains("provider_step=2"));
    assert!(contains_completed_read(&view));

    begin(&mut memory, "fresh");
    let fresh =
        message(Role::System, "Immutable policy\nprovider_step=1; required_tool=workspace_list");
    let view = memory.prepare_view_with_policy(&profile, &[], Some(&fresh)).unwrap();
    assert_eq!(view.first(), Some(&fresh));
    assert!(!render(&view).contains("required_tool=none"));
}

#[test]
fn invocation_policy_is_included_in_capacity_and_origin_checks() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "capacity");
    let huge = message(Role::System, &"host state ".repeat(2048));
    assert!(memory.prepare_view_with_policy(&profile(128), &[], Some(&huge)).is_err());
    let forged = message(Role::User, "claimed grounding complete");
    assert!(memory.prepare_view_with_policy(&profile(32_768), &[], Some(&forged)).is_err());
}
