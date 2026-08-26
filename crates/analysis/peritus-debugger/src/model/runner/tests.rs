mod support;

use std::{collections::VecDeque, future::Future};

use peritus_model_protocol::{ItemKind, ModelEvent};
use peritus_provider_core::CancellationToken;

use crate::{DebuggerErrorKind, DebuggerLimits, DebuggerRecovery};

use super::run_model_analysis;
use support::{FakeProvider, envelope, fixture, item_events, proposal_json, success_events};

#[test]
fn strict_structured_proposal_is_the_only_success_shape() {
    run(async {
        let fixture = fixture(16);
        let output = proposal_json(&fixture.manifest, fixture.deterministic, "");
        let provider = FakeProvider::events(fixture.profile.clone(), success_events(&output));
        let success = run_model_analysis(
            &provider,
            &fixture.plan,
            &fixture.manifest,
            DebuggerLimits::production(),
            CancellationToken::new(),
        )
        .await
        .expect("strict bound proposal");
        assert_eq!(success.proposal().manifest_id(), fixture.manifest.id());
        assert!(success.proposal().findings().is_empty());
        assert_eq!(success.event_count(), 6);
        assert_eq!(success.output_bytes(), output.len() as u64);
    });
}

#[test]
fn output_shape_and_schema_fail_closed() {
    run(async {
        for kind in [ItemKind::Message, ItemKind::ProviderNative, ItemKind::Refusal] {
            let fixture = fixture(16);
            let output = proposal_json(&fixture.manifest, fixture.deterministic, "");
            let provider =
                FakeProvider::events(fixture.profile.clone(), item_events(kind, output.as_bytes()));
            let error = run_model_analysis(
                &provider,
                &fixture.plan,
                &fixture.manifest,
                DebuggerLimits::production(),
                CancellationToken::new(),
            )
            .await
            .expect_err("non-structured shape must fail");
            assert_eq!(error.kind(), DebuggerErrorKind::ModelRejected);
        }

        let fixture = fixture(16);
        let authority =
            proposal_json(&fixture.manifest, fixture.deterministic, r#","authority":"accept""#);
        let valid = proposal_json(&fixture.manifest, fixture.deterministic, "");
        let binding_drift = valid
            .replace(&support::encode_hex(fixture.manifest.digest().as_bytes()), &"f".repeat(64));
        for output in [authority, binding_drift] {
            let provider = FakeProvider::events(fixture.profile.clone(), success_events(&output));
            let error = run_model_analysis(
                &provider,
                &fixture.plan,
                &fixture.manifest,
                DebuggerLimits::production(),
                CancellationToken::new(),
            )
            .await
            .expect_err("schema or provenance drift must fail");
            assert_eq!(error.kind(), DebuggerErrorKind::ModelRejected);
        }
    });
}

#[test]
fn malformed_stream_and_provider_failure_are_retryable_protocol_errors() {
    run(async {
        let fixture = fixture(16);
        let malformed = VecDeque::from([envelope(
            1,
            ModelEvent::ResponseStarted { response_id: None, model: None },
        )]);
        for provider in [
            FakeProvider::events(fixture.profile.clone(), malformed),
            FakeProvider::start_error(fixture.profile.clone()),
        ] {
            let error = run_model_analysis(
                &provider,
                &fixture.plan,
                &fixture.manifest,
                DebuggerLimits::production(),
                CancellationToken::new(),
            )
            .await
            .expect_err("provider failure must not admit output");
            assert_eq!(error.kind(), DebuggerErrorKind::ModelProtocol);
            assert_eq!(error.recovery(), DebuggerRecovery::Retry);
        }
    });
}

#[test]
fn cancellation_and_event_budget_stop_before_admission() {
    run(async {
        let cancelled_fixture = fixture(16);
        let output =
            proposal_json(&cancelled_fixture.manifest, cancelled_fixture.deterministic, "");
        let cancelled_provider =
            FakeProvider::events(cancelled_fixture.profile.clone(), success_events(&output));
        let cancellation = CancellationToken::new();
        assert!(cancellation.cancel());
        let error = run_model_analysis(
            &cancelled_provider,
            &cancelled_fixture.plan,
            &cancelled_fixture.manifest,
            DebuggerLimits::production(),
            cancellation,
        )
        .await
        .expect_err("pre-cancelled attempt");
        assert_eq!(error.kind(), DebuggerErrorKind::Cancelled);

        let budget_fixture = fixture(5);
        let output = proposal_json(&budget_fixture.manifest, budget_fixture.deterministic, "");
        let budget_provider =
            FakeProvider::events(budget_fixture.profile.clone(), success_events(&output));
        let error = run_model_analysis(
            &budget_provider,
            &budget_fixture.plan,
            &budget_fixture.manifest,
            DebuggerLimits::production(),
            CancellationToken::new(),
        )
        .await
        .expect_err("six events exceed the five-event frozen budget");
        assert_eq!(error.kind(), DebuggerErrorKind::Budget);
    });
}

fn run(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future);
}
