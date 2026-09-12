//! Exact assistant reasoning replay stays separate from authoritative working entries.

use super::support::*;
use peritus_agent::DeveloperToolObservation;
use peritus_model_protocol::{
    ContentBlock, Message, ProtocolLimits, ReasoningReplay, Role, ToolResult,
};

#[test]
fn assistant_reasoning_survives_local_archive_and_recovery_without_becoming_instructions() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "reasoning");
    let limits = ProtocolLimits::PRODUCTION;
    let replay = ContentBlock::Reasoning(
        ReasoningReplay::new(
            None,
            br#"{"service":"opencode-zen","fields":{"reasoning_content":"untrusted opaque note"}}"#
                .to_vec(),
            limits,
        )
        .unwrap(),
    );
    let mut blocks = message(Role::Assistant, "visible answer").content().to_vec();
    blocks.insert(0, replay.clone());
    let assistant = Message::new(Role::Assistant, blocks, limits).unwrap();
    memory.observe_message(&assistant).expect("assistant replay can be archived");
    let tool = call("reasoning-call");
    let proposal = Message::new(
        Role::Assistant,
        vec![replay.clone(), ContentBlock::ToolCall(tool.clone())],
        limits,
    )
    .unwrap();
    memory.observe_message(&proposal).unwrap();
    let output = canonical(&serde_json::json!({"content":"observed"}));
    memory
        .observe_tool(&tool, &DeveloperToolObservation { output: output.clone(), is_error: false })
        .unwrap();
    let result = Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult(ToolResult::new(tool.id().clone(), output, false))],
        limits,
    )
    .unwrap();
    memory.observe_message(&result).unwrap();
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    assert!(view.contains(&assistant));
    let position = view.iter().position(|message| message == &proposal).unwrap();
    assert_eq!(view.get(position + 1), Some(&result));
    assert!(!render(&view).contains("untrusted opaque note"));
    assert!(memory.state.entries(memory.state.binding()).unwrap().is_empty());
    memory.publish(&view).unwrap();
    drop(memory);

    let mut memory = fixture.open();
    let recovered = memory.prepare_view(&profile(32_768), &[]).unwrap();
    assert!(recovered.contains(&assistant));
    let position = recovered.iter().position(|message| message == &proposal).unwrap();
    assert_eq!(recovered.get(position + 1), Some(&result));
    for role in [Role::System, Role::Developer, Role::User, Role::Tool] {
        assert!(
            Message::new(role, vec![replay.clone()], limits).is_err(),
            "opaque content from {role:?}"
        );
    }
    begin(&mut memory, "next-invocation");
    let fresh = memory.prepare_view(&profile(32_768), &[]).unwrap();
    assert!(
        !fresh
            .iter()
            .flat_map(Message::content)
            .any(|block| matches!(block, ContentBlock::Reasoning(_)))
    );
}
