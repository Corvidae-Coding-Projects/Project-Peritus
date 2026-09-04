use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use peritus_model_protocol::{
    FailureCategory, ModelFailure, ModelRequest, OutcomeCertainty, ProviderName,
    RedactedDiagnostic, Retryability, TransportPhase,
};
use peritus_product_runner::{ProductRunner, RoleProviders};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProviderAvailability,
    ProviderCandidate, ProviderCoreError, ProviderQualification, ProviderRecoveryDisposition,
    ProviderRequirement, ProviderTerminal, ProviderTerminalCause, select_qualified_provider,
};
use serde::Deserialize;

use super::{
    fixture::{Expected, FixtureSet},
    product_fixture::{clean_review, complete_writer, input, profile, repository, scripted},
};

const CASES: &str = include_str!("../../tests/fixtures/general-capability/provider/cases.json");

#[derive(Deserialize)]
struct Case {
    name: String,
    expected: Expected,
}

struct UnavailableProvider {
    profile: peritus_model_protocol::ProviderProfile,
    starts: Arc<AtomicUsize>,
}

impl ModelProvider for UnavailableProvider {
    fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        Box::pin(async { Err(ProviderCoreError::credential("fixture credential is unavailable")) })
    }
}

struct QualifiedProvider {
    profile: peritus_model_protocol::ProviderProfile,
    availability: ProviderAvailability,
}

impl ModelProvider for QualifiedProvider {
    fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        &self.profile
    }

    fn availability(&self) -> ProviderAvailability {
        self.availability
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async { Err(ProviderCoreError::configuration("fixture", "not invoked")) })
    }
}

#[test]
fn provider_capability_and_recovery_matrix_is_typed() {
    let fixtures: FixtureSet<Case> = serde_json::from_str(CASES).expect("provider fixtures");
    assert_eq!(fixtures.cases.len(), 3);
    assert_eq!(fixtures.cases[0].expected, Expected::Success);
    assert_eq!(fixtures.cases[1].expected, Expected::Partial);
    assert_eq!(fixtures.cases[2].expected, Expected::Failure);
    assert!(fixtures.cases.iter().all(|case| !case.name.is_empty()));

    let requirement = ProviderRequirement::new(true, 100_000, true).expect("requirement");
    let incapable = QualifiedProvider {
        profile: profile([0x31; 16], "no-image"),
        availability: ProviderAvailability::LiveCanary,
    };
    let capable = image_profile(0x32, 200_000, ProviderAvailability::LiveCanary);
    assert!(
        ProviderQualification::evaluate(incapable.profile(), incapable.availability(), requirement)
            .is_err()
    );
    let (selected, qualification) = select_qualified_provider(
        ProviderCandidate::new(&incapable, true),
        &[ProviderCandidate::new(&capable, true)],
        requirement,
    )
    .expect("authorized capable fallback");
    assert_eq!(selected.profile().profile_id(), capable.profile().profile_id());
    assert!(qualification.image_input());

    assert!(
        select_qualified_provider(
            ProviderCandidate::new(&incapable, true),
            &[ProviderCandidate::new(&capable, false)],
            requirement,
        )
        .is_err()
    );
    let small_context = image_profile(0x33, 32_000, ProviderAvailability::LiveCanary);
    assert!(
        ProviderQualification::evaluate(
            small_context.profile(),
            small_context.availability(),
            requirement
        )
        .is_err()
    );

    assert_terminal(
        ProviderTerminal::empty_response(),
        ProviderTerminalCause::EmptyResponse,
        ProviderRecoveryDisposition::RetrySameRoute,
    );
    assert_terminal(
        ProviderTerminal::from_model_failure(&failure(
            FailureCategory::InvalidRequest,
            OutcomeCertainty::Terminal,
            "provider.context_overflow",
        )),
        ProviderTerminalCause::ContextOverflow,
        ProviderRecoveryDisposition::CompactThenRetry,
    );
    assert_terminal(
        ProviderTerminal::from_model_failure(&failure(
            FailureCategory::Authentication,
            OutcomeCertainty::Terminal,
            "provider.authentication",
        )),
        ProviderTerminalCause::Authentication,
        ProviderRecoveryDisposition::AwaitCredentialRepair,
    );
}

#[test]
fn a_failed_primary_is_circuit_bypassed_while_authorized_fallbacks_finish() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(
        async {
            let repository = repository();
            let state = tempfile::tempdir().expect("state");
            let starts = Arc::new(AtomicUsize::new(0));
            let unavailable: Arc<dyn ModelProvider> = Arc::new(UnavailableProvider {
                profile: profile([0x41; 16], "unavailable-primary"),
                starts: Arc::clone(&starts),
            });
            let implementer: Arc<dyn ModelProvider> =
                scripted(0x42, "fallback-writer", complete_writer());
            let reviewer: Arc<dyn ModelProvider> =
                scripted(0x43, "fallback-reviewer", clean_review());
            let failovers = Arc::new(Mutex::new(Vec::new()));
            let observed = Arc::clone(&failovers);
            let outcome = ProductRunner::run(
                input(
                    &repository,
                    &state,
                    0x44,
                    0x45,
                    RoleProviders {
                        writer: Arc::clone(&unavailable),
                        reviewer: Arc::clone(&unavailable),
                        fixer: unavailable,
                        fallbacks: vec![implementer, reviewer],
                    },
                    None,
                ),
                Arc::new(move |update| {
                    observed
                        .lock()
                        .expect("provider observations")
                        .push(update.progress.provider_failovers());
                }),
            )
            .await
            .expect("fallback-backed run");
            assert!(outcome.settlement().is_accepted());
            assert_eq!(starts.load(Ordering::Acquire), 1, "open primary circuit was retried");
            assert!(
                failovers
                    .lock()
                    .expect("provider observations")
                    .last()
                    .is_some_and(|count| *count >= 3)
            );
        },
    );
}

fn image_profile(seed: u8, context: u64, availability: ProviderAvailability) -> QualifiedProvider {
    use peritus_model_protocol::{
        CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits,
        ModelName, OutputLimitEnforcement, ProviderProfile, ResumeKind, StateMode, WireDialect,
    };
    use peritus_types::ProviderProfileId;

    let profile = ProviderProfile::new(
        ProviderProfileId::new([seed; 16]).expect("profile id"),
        1,
        ProviderName::new(format!("provider-{seed}")).expect("provider"),
        ModelName::new(format!("model-{seed}")).expect("model"),
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ImageInput], &[])
            .expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(context, 4_096, 16, 1, 512 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile");
    QualifiedProvider { profile, availability }
}

fn failure(category: FailureCategory, certainty: OutcomeCertainty, code: &str) -> ModelFailure {
    ModelFailure::new(
        ProviderName::new("fixture".to_owned()).expect("provider"),
        category,
        TransportPhase::Completed,
        certainty,
        Retryability::Never,
        None,
        None,
        None,
        RedactedDiagnostic::new(code.to_owned(), None, None, None).expect("diagnostic"),
    )
}

fn assert_terminal(
    terminal: ProviderTerminal,
    cause: ProviderTerminalCause,
    recovery: ProviderRecoveryDisposition,
) {
    assert_eq!(terminal.cause(), cause);
    assert_eq!(terminal.recovery(), recovery);
}
