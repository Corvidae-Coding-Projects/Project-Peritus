use peritus_app_protocol::{
    AppProtocolLimits, CorrelationId, RemainingWorkKind, RequestId, ShutdownCompletionDisposition,
    ShutdownPhase, ShutdownRequest,
};

use super::{
    ShutdownBounds, ShutdownCoordinator, ShutdownStage, ShutdownTrigger, ShutdownWorkCounts,
};

#[test]
fn bounds_preserve_exact_aggregate_reporting() {
    assert!(ShutdownBounds::new(0, 64).is_err());
    assert!(ShutdownBounds::new(7, 64).is_err());
    assert!(ShutdownBounds::new(8, 1).is_err());
    assert!(ShutdownBounds::from_protocol(AppProtocolLimits::PRODUCTION).is_ok());
}

#[test]
fn client_shutdown_reports_monotonic_exact_unclean_truth() {
    let request = request();
    let mut coordinator = ShutdownCoordinator::begin(
        Some(request),
        ShutdownBounds::new(8, 64).expect("valid shutdown bounds"),
    )
    .expect("client shutdown begins");
    assert_eq!(coordinator.trigger(), ShutdownTrigger::Client(request));
    assert!(matches!(
        coordinator.protocol_state().map(|state| state.phase()),
        Some(ShutdownPhase::Accepted(accepted)) if accepted.request() == request
    ));

    let first = ShutdownWorkCounts::empty()
        .with_requests(2)
        .with_subscriptions(1)
        .with_artifact_transfers(3)
        .with_terminal_attachments(4)
        .with_workers(5)
        .with_processes(6)
        .with_outbox(7)
        .with_indeterminate_effects(8);
    let progress = coordinator
        .record_stage(ShutdownStage::AdmissionClosed, first)
        .expect("first named stage")
        .expect("client progress");
    assert_eq!(progress.completed_steps(), 1);
    assert_eq!(progress.total_steps(), 6);
    assert_eq!(progress.remaining().len(), 8);
    assert_eq!(progress.remaining()[0].kind(), RemainingWorkKind::Request);
    assert_eq!(progress.remaining()[0].descriptor(), "requests=2");
    assert_eq!(progress.remaining()[7].kind(), RemainingWorkKind::Other);
    assert_eq!(progress.remaining()[7].descriptor(), "indeterminate-effects=8");

    coordinator
        .record_stage(ShutdownStage::WorkersJoined, first.with_workers(0))
        .expect("unused stages may be skipped monotonically");
    assert!(coordinator.record_stage(ShutdownStage::ConnectionsJoined, first).is_err());
    assert!(coordinator.complete(first).is_err());

    coordinator.record_stage(ShutdownStage::AuthorityStopped, first).expect("final named stage");
    let outcome = coordinator.complete(first).expect("truthful unclean completion");
    assert_eq!(outcome.disposition(), ShutdownCompletionDisposition::Unclean);
    assert_eq!(outcome.remaining().len(), 8);
    let complete = outcome.protocol().expect("client completion is correlated");
    assert_eq!(complete.request(), request);
    assert_eq!(complete.disposition(), ShutdownCompletionDisposition::Unclean);
    assert_eq!(coordinator.complete(first).expect("identical completion is idempotent"), outcome,);
    assert!(coordinator.complete(ShutdownWorkCounts::empty()).is_err());
}

#[test]
fn operating_system_shutdown_is_clean_without_synthetic_protocol_identity() {
    let mut coordinator = ShutdownCoordinator::begin(
        None,
        ShutdownBounds::new(8, 64).expect("valid shutdown bounds"),
    )
    .expect("signal shutdown begins");
    assert_eq!(coordinator.trigger(), ShutdownTrigger::OperatingSystemSignal);
    assert_eq!(coordinator.protocol_state(), None);
    assert_eq!(coordinator.stage(), None);
    assert_eq!(coordinator.counts(), ShutdownWorkCounts::empty());
    assert!(coordinator.remaining().is_empty());

    let progress = coordinator
        .record_stage(ShutdownStage::AuthorityStopped, ShutdownWorkCounts::empty())
        .expect("signal shutdown reaches final stage");
    assert_eq!(progress, None);
    assert_eq!(ShutdownStage::AuthorityStopped.name(), "authority-stopped");
    let outcome =
        coordinator.complete(ShutdownWorkCounts::empty()).expect("zero counts complete cleanly");
    assert_eq!(outcome.disposition(), ShutdownCompletionDisposition::Clean);
    assert!(outcome.remaining().is_empty());
    assert_eq!(outcome.protocol(), None);
}

fn request() -> ShutdownRequest {
    ShutdownRequest::new(
        RequestId::new([1; 16]).expect("nonzero request identity"),
        CorrelationId::new([2; 16]).expect("nonzero correlation identity"),
    )
}
