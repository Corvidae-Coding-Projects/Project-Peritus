//! Resumed writer invocations must not reuse the prior invocation's effect receipts.

use super::support::{
    list_arguments, named_tool_response, read_arguments, tool_response, write_arguments,
};
use super::*;

#[test]
fn resumed_writer_can_issue_a_different_first_effect_at_the_same_conversation_revision() {
    resumed_writer(false);
}

#[test]
fn resumed_writer_keeps_distinct_effects_with_local_memory_disabled() {
    resumed_writer(true);
}

fn resumed_writer(disable_memory: bool) {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x91, "interrupted-writer", partial_writer(INCORRECT));
        let first = ProductRunner::run(
            memory_policy(
                input(
                    &repository,
                    &state,
                    0x92,
                    0x93,
                    roles(writer.clone(), writer.clone(), writer),
                    Arc::new(AtomicBool::new(false)),
                    SCENARIO_TIMEOUT,
                    None,
                ),
                disable_memory,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("first invocation retains its effect");
        assert_eq!(first.settlement().cause(), SettlementCause::Provider);
        let resume = first.resume().expect("writer continuation");
        assert_eq!(resume.next_phase(), ProductRunPhase::Writing);
        let encoded = resume.encode_durable().expect("durable continuation");
        let resume = peritus_product_runner::ProductRunResume::decode_durable(
            &encoded,
            &format!("User:\n{TASK}"),
        )
        .expect("restored continuation");
        let writer = scripted(
            0x94,
            "resumed-writer",
            vec![
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                tool_response(write_arguments("src/lib.rs", CORRECT)),
            ],
        );
        let resumed = ProductRunner::run(
            memory_policy(
                input(
                    &repository,
                    &state,
                    0x92,
                    0x93,
                    roles(writer.clone(), writer.clone(), writer),
                    Arc::new(AtomicBool::new(false)),
                    SCENARIO_TIMEOUT,
                    Some(resume),
                ),
                disable_memory,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("resumed invocation settles");
        assert_eq!(resumed.settlement().cause(), SettlementCause::Provider);
        assert_eq!(
            std::fs::read_to_string(repository.path().join("src/lib.rs")).expect("current effect"),
            CORRECT,
            "the resumed request must not collide with the earlier invocation's first write",
        );
    });
}

fn memory_policy(
    mut input: peritus_product_runner::ProductRunInput,
    disable_memory: bool,
) -> peritus_product_runner::ProductRunInput {
    input.command_runtime = input
        .command_runtime
        .with_local_context(peritus_product_runner::LocalContextConfig {
            enabled: !disable_memory,
            ..Default::default()
        })
        .expect("local memory policy");
    input
}
