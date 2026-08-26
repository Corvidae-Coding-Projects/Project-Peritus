use crate::support::{command_binding, event_fixture_bytes, fixture_id, revision};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppProtocolLimits, ArtifactChunk, ArtifactMetadata,
    ArtifactTransferState, CanonicalMediaType, CommandResult, CommittedEventRange, DaemonReadiness,
    DaemonStatus, DeliveryAdmission, DeliveryAttemptId, EventCursor, IdempotencyAdmission,
    IdempotencyWindow, IncompatibilityReason, NegotiationOutcome, PromptAnswer,
    PromptAnswerPayload, PromptBinding, PromptChoice, PromptConstraint, PromptCorrelation,
    PromptId, PromptKind, PromptState, ProtocolFeatureName, ProtocolId, ProtocolVersion,
    RegisteredEventFrame, RequestId, RetryDisposition, ServerCapabilities, ShutdownAccepted,
    ShutdownComplete, ShutdownCompletionDisposition, ShutdownRequest, ShutdownState,
    SubscriptionFilter, SubscriptionGap, SubscriptionId, SubscriptionPhase, SubscriptionState,
    TerminalAttachmentId, TerminalBinding, TerminalExit, TerminalExitDisposition, TerminalOutput,
    TerminalState, TerminalStream, TransferId, UserInputValue, VersionRange,
    WellKnownProtocolFeature, decode_app_message, negotiate,
};
use peritus_conformance::{
    ProtocolConformanceFixture, ProtocolConformanceObservation, ProtocolScenario,
};
use peritus_types::{ActorId, ArtifactId, EventId, Generation, ProcessId, SessionId, Sha256Digest};

pub fn observe(fixture: &ProtocolConformanceFixture) -> ProtocolConformanceObservation {
    use ProtocolScenario as P;
    let scenario = fixture.scenario();
    let exact = match scenario {
        P::NegotiationExact
        | P::NegotiationDowngraded
        | P::NegotiationIncompatible
        | P::RequiredFeature => negotiation(scenario),
        P::CommandBinding => command_binding_exact(),
        P::Idempotency => idempotency_exact(),
        P::SubscriptionResume => subscription_resume_exact(),
        P::AckLegality => acknowledgement_exact(),
        P::GapSnapshot => gap_exact(),
        P::Backpressure => backpressure_exact(),
        P::ArtifactTransfer => artifact_exact(),
        P::PromptFreshness => prompt_exact(),
        P::TerminalOrdering => terminal_exact(),
        P::DaemonLifecycle => daemon_exact(),
        P::MalformedInput => malformed_rejected(),
        P::Bounds => bounds_enforced(fixture),
    };
    ProtocolConformanceObservation {
        expected_terminal: exact,
        negotiation_exact: exact
            && matches!(
                scenario,
                P::NegotiationExact
                    | P::NegotiationDowngraded
                    | P::NegotiationIncompatible
                    | P::RequiredFeature
            ),
        command_binding_exact: exact && scenario == P::CommandBinding,
        idempotency_exact: exact && scenario == P::Idempotency,
        delivery_exact: exact && matches!(scenario, P::SubscriptionResume | P::AckLegality),
        flow_control_exact: exact && matches!(scenario, P::GapSnapshot | P::Backpressure),
        artifact_exact: exact && scenario == P::ArtifactTransfer,
        prompt_exact: exact && scenario == P::PromptFreshness,
        terminal_exact: exact && scenario == P::TerminalOrdering,
        daemon_control_exact: exact && scenario == P::DaemonLifecycle,
        malformed_rejected: exact && scenario == P::MalformedInput,
        bounds_enforced: exact && scenario == P::Bounds,
        stable_error_exact: stable_error_exact(),
        non_authoritative: true,
    }
}

fn negotiation(scenario: ProtocolScenario) -> bool {
    let feature = ProtocolFeatureName::well_known(WellKnownProtocolFeature::ArtifactTransfer)
        .expect("well-known feature");
    let client = peritus_app_protocol::ClientHello::new(
        fixture_id(1, ProtocolId::new),
        vec![VersionRange::new(1, 0, 2).expect("version range")],
        if scenario == ProtocolScenario::RequiredFeature {
            vec![feature.clone()]
        } else {
            Vec::new()
        },
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "a2-client".to_owned(),
    )
    .expect("client hello");
    let (ranges, features) = match scenario {
        ProtocolScenario::NegotiationDowngraded => {
            (vec![VersionRange::new(1, 0, 1).expect("version range")], vec![feature])
        }
        ProtocolScenario::NegotiationIncompatible => {
            (vec![VersionRange::new(2, 0, 1).expect("version range")], Vec::new())
        }
        ProtocolScenario::RequiredFeature => {
            (vec![VersionRange::new(1, 0, 2).expect("version range")], Vec::new())
        }
        _ => (vec![VersionRange::new(1, 0, 2).expect("version range")], vec![feature]),
    };
    let server = ServerCapabilities::new(
        ranges,
        features,
        AppProtocolLimits::PRODUCTION,
        "a2-server".to_owned(),
    )
    .expect("server capabilities");
    match (scenario, negotiate(&client, &server).expect("negotiation observation").outcome()) {
        (ProtocolScenario::NegotiationExact, NegotiationOutcome::Compatible(value)) => {
            value.version() == ProtocolVersion::new(1, 2).expect("version")
        }
        (ProtocolScenario::NegotiationDowngraded, NegotiationOutcome::Downgraded(value)) => {
            value.version() == ProtocolVersion::new(1, 1).expect("version")
        }
        (
            ProtocolScenario::NegotiationIncompatible,
            NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
        ) => true,
        (
            ProtocolScenario::RequiredFeature,
            NegotiationOutcome::Incompatible(IncompatibilityReason::MissingRequiredFeatures(value)),
        ) => value.len() == 1,
        _ => false,
    }
}

fn command_binding_exact() -> bool {
    let binding = command_binding(40, b"a2-command");
    binding.actor_id() == fixture_id(40, ActorId::new)
        && binding.session_id() == fixture_id(41, SessionId::new)
        && binding.expected_revision() == Some(revision(5))
        && !binding.frames().envelope_frame().bytes().is_empty()
        && !binding.frames().command_frame().bytes().is_empty()
}

fn idempotency_exact() -> bool {
    let binding = command_binding(41, b"a2-idempotency");
    let range = CommittedEventRange::new(EventCursor::new(1), EventCursor::new(2)).unwrap();
    let mut window = IdempotencyWindow::new(1).unwrap();
    let new = window.admit(&binding) == IdempotencyAdmission::New;
    window
        .record(&binding, CommandResult::committed(binding.request_id(), range))
        .expect("record result");
    new && matches!(window.admit(&binding), IdempotencyAdmission::Replay { .. })
}

fn subscription() -> (SubscriptionId, SubscriptionState) {
    let id = fixture_id(50, SubscriptionId::new);
    let filter = SubscriptionFilter::new(vec!["run.events".to_owned()], 4, 64).unwrap();
    (id, SubscriptionState::new(id, filter, EventCursor::new(4), 1).unwrap())
}

fn delivery(
    state: &mut SubscriptionState,
    event: u8,
    attempt: u8,
) -> peritus_app_protocol::Delivery {
    let frame =
        RegisteredEventFrame::new(event_fixture_bytes(), AppProtocolLimits::PRODUCTION.codec())
            .unwrap();
    match state
        .deliver(
            fixture_id(event, EventId::new),
            fixture_id(attempt, DeliveryAttemptId::new),
            frame,
        )
        .unwrap()
    {
        DeliveryAdmission::Delivered(value) => value,
        DeliveryAdmission::Backpressured => panic!("empty subscription window"),
    }
}

fn subscription_resume_exact() -> bool {
    let (_, mut state) = subscription();
    let first = delivery(&mut state, 51, 52);
    let retry = state.redeliver(first.cursor(), fixture_id(53, DeliveryAttemptId::new)).unwrap();
    retry.event_id() == first.event_id()
        && retry.frame().bytes() == first.frame().bytes()
        && retry.attempt() == 2
}

fn acknowledgement_exact() -> bool {
    let (id, mut state) = subscription();
    let first = delivery(&mut state, 54, 55);
    state.acknowledge(peritus_app_protocol::Acknowledgement::new(id, first.cursor())) == Ok(1)
        && state
            .acknowledge(peritus_app_protocol::Acknowledgement::new(id, EventCursor::new(3)))
            .is_err()
}

fn gap_exact() -> bool {
    let (_, mut state) = subscription();
    let gap = SubscriptionGap::new(EventCursor::new(4), EventCursor::new(6), EventCursor::new(9))
        .unwrap();
    state.declare_gap(gap).is_ok()
        && matches!(state.phase(), SubscriptionPhase::SnapshotRequired(value) if value == gap)
}

fn backpressure_exact() -> bool {
    let (_, mut state) = subscription();
    let _ = delivery(&mut state, 56, 57);
    let frame =
        RegisteredEventFrame::new(event_fixture_bytes(), AppProtocolLimits::PRODUCTION.codec())
            .unwrap();
    state.deliver(fixture_id(58, EventId::new), fixture_id(59, DeliveryAttemptId::new), frame)
        == Ok(DeliveryAdmission::Backpressured)
}

fn artifact_exact() -> bool {
    let digest = Sha256Digest::new([60; 32]);
    let metadata = ArtifactMetadata::new(
        fixture_id(61, TransferId::new),
        fixture_id(62, ArtifactId::new),
        2,
        CanonicalMediaType::new("text/plain".to_owned(), 64).unwrap(),
        digest,
        2,
        2,
    )
    .unwrap();
    let chunk =
        ArtifactChunk::new(metadata.transfer_id(), metadata.artifact_id(), 0, 0, b"ok".to_vec(), 2)
            .unwrap();
    let mut state = ArtifactTransferState::new(metadata, 2).unwrap();
    state.accept_chunk(&chunk) == Ok(2) && state.complete(digest).is_ok()
}

fn prompt_exact() -> bool {
    let correlation = PromptCorrelation::new(
        fixture_id(63, RequestId::new),
        fixture_id(64, PromptId::new),
        fixture_id(65, SessionId::new),
        fixture_id(66, ActorId::new),
        revision(5),
        Sha256Digest::new([67; 32]),
        Generation::new(1).unwrap(),
    );
    let binding = PromptBinding::new(
        PromptKind::UserInput,
        correlation,
        vec![PromptChoice::new("yes".to_owned(), "Yes".to_owned(), 8, 8).unwrap()],
        vec![PromptConstraint::BoundChoiceOnly],
        2,
        2,
    )
    .unwrap();
    let answer = PromptAnswer::new(
        correlation,
        PromptAnswerPayload::UserInput(UserInputValue::selection("yes".to_owned(), 8).unwrap()),
        16,
    )
    .unwrap();
    let mut state = PromptState::new(binding, 16).unwrap();
    state.answer(answer, revision(5)).is_ok()
}

fn terminal_exact() -> bool {
    let binding = TerminalBinding::new(
        fixture_id(68, TerminalAttachmentId::new),
        fixture_id(69, ProcessId::new),
        fixture_id(70, RequestId::new),
    );
    let output =
        TerminalOutput::new(binding, 0, 0, TerminalStream::Stdout, b"ok".to_vec(), 8).unwrap();
    let mut state = TerminalState::new(binding, 8).unwrap();
    state.accept_output(&output).is_ok()
        && state.exit(TerminalExit::new(binding, 1, 2, TerminalExitDisposition::Code(0))).is_ok()
}

fn daemon_exact() -> bool {
    let status =
        DaemonStatus::new(DaemonReadiness::ReadyReadOnly, Some("safe".to_owned()), 16).unwrap();
    let request = ShutdownRequest::new(
        fixture_id(71, RequestId::new),
        fixture_id(72, peritus_app_protocol::CorrelationId::new),
    );
    let mut state = ShutdownState::running();
    state.request(request).is_ok()
        && state.accept(ShutdownAccepted::new(request)).is_ok()
        && state
            .complete(
                ShutdownComplete::new(request, ShutdownCompletionDisposition::Clean, Vec::new(), 2)
                    .unwrap(),
            )
            .is_ok()
        && !status.mutation_ready()
}

fn malformed_rejected() -> bool {
    decode_app_message(b"PRTS", AppProtocolLimits::PRODUCTION)
        .is_err_and(|error| error.code() == AppErrorCode::TruncatedFrame)
}

fn bounds_enforced(fixture: &ProtocolConformanceFixture) -> bool {
    fixture.maximum_features() > 0
        && fixture.maximum_in_flight() > 0
        && fixture.maximum_chunk_bytes() > 0
        && fixture.maximum_frame_bytes() > 0
        && SubscriptionFilter::new(vec!["a".to_owned(), "b".to_owned()], 1, 8).is_err()
}

fn stable_error_exact() -> bool {
    let error = AppProtocolError::new(AppErrorCode::Backpressure, None);
    error.code() == AppErrorCode::Backpressure
        && error.retry() == RetryDisposition::AfterRecovery
        && error.diagnostic().is_none()
}
