//! Real process tests of final-output binding and failure metadata, without remote requests.
use super::support::{self, Probe};
use peritus_conformance::ProviderScenario;
use peritus_model_protocol::{
    FailureCategory, ModelEvent, ModelName, ProviderProfile, Retryability,
};

#[test]
fn final_artifact_and_full_stream_are_checked_before_emitting_any_host_call() {
    for (model, tools, expected) in [
        ("commentary", false, None),
        ("missing-final", false, Some("openai.codex_runtime.invalid_envelope")),
        ("bad-arguments", true, Some("openai.codex_runtime.invalid_tool_arguments")),
        ("native-tool", false, Some("openai.codex_runtime.native_tool")),
    ] {
        let scenario = ProviderScenario::UsageAccounting;
        let base = support::profile(scenario, 0xe1).unwrap();
        let profile = ProviderProfile::new(
            base.profile_id(),
            1,
            base.provider().clone(),
            ModelName::new(model.to_owned()).unwrap(),
            base.dialect(),
            base.capabilities(),
            base.provenance(),
            base.limits(),
            base.output_limit_enforcement(),
            base.state_mode(),
            base.resume_kind(),
            base.cancellation_kind(),
        )
        .unwrap();
        let request = support::request(&profile, tools, None).unwrap();
        let probe = Probe::run_request(scenario, profile, request).unwrap();
        assert_eq!(probe.turn_requests(), 1, "no hidden adapter retry");
        assert!(probe.directory_removed, "private final artifact is removed");
        if let Some(expected) = expected {
            let Some(ModelEvent::ResponseFailed(failure)) =
                probe.events.last().map(peritus_model_protocol::EventEnvelope::event)
            else {
                panic!("expected {expected}");
            };
            assert_eq!(failure.diagnostic().code(), expected);
            assert!(
                !probe
                    .events
                    .iter()
                    .any(|event| matches!(event.event(), ModelEvent::ToolCallStarted { .. })),
                "invalid batch cannot emit a partial host proposal"
            );
            if model == "native-tool" {
                assert_eq!(failure.category(), FailureCategory::Safety);
                assert_eq!(failure.retryability(), Retryability::Never);
            } else {
                assert!(probe.events.iter().any(|event| matches!(event.event(), ModelEvent::Usage(usage) if usage.counters().total_tokens() == Some(17))), "known usage survives a rejected response");
            }
        } else {
            assert!(probe.completed(), "preliminary prose must not reject the valid final result");
        }
    }
}
