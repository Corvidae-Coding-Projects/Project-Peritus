//! Daemon readiness, heartbeat, diagnostic, and shutdown integration tests.

mod support;

use peritus_app_protocol::{
    CorrelationId, DaemonControlErrorKind, DaemonHeartbeat, DaemonReadiness, DaemonStatus,
    HeartbeatId, HeartbeatState, RemainingWork, RemainingWorkKind, RequestId, ShutdownAccepted,
    ShutdownComplete, ShutdownCompletionDisposition, ShutdownPhase, ShutdownProgress,
    ShutdownRequest, ShutdownState,
};
use support::fixture_id;

#[allow(
    clippy::too_many_lines,
    reason = "one sequential trace keeps readiness, heartbeat, and shutdown state causality visible"
)]
#[test]
fn readiness_heartbeat_diagnostics_and_shutdown_report_truthful_states() {
    let read_write =
        DaemonStatus::new(DaemonReadiness::ReadyReadWrite, None, 64).expect("read-write status");
    let read_only = DaemonStatus::new(
        DaemonReadiness::ReadyReadOnly,
        Some("migration required".to_owned()),
        64,
    )
    .expect("bounded diagnostic status");
    assert!(read_write.mutation_ready());
    assert!(!read_only.mutation_ready());
    assert_eq!(read_only.diagnostic(), Some("migration required"));
    assert!(!DaemonReadiness::Starting.mutation_ready());
    assert!(!DaemonReadiness::Draining.mutation_ready());
    assert!(!DaemonReadiness::Unavailable.diagnostic_ready());
    assert_eq!(
        DaemonStatus::new(DaemonReadiness::Unavailable, Some("too long".to_owned()), 3)
            .expect_err("diagnostic bound is enforced")
            .kind(),
        DaemonControlErrorKind::InvalidInput,
    );

    let mut heartbeats = HeartbeatState::new(5);
    let first = DaemonHeartbeat::new(fixture_id(10, HeartbeatId::new), 5, read_only.clone());
    heartbeats.observe(first).expect("first expected heartbeat");
    assert_eq!(heartbeats.next_sequence(), 6);
    assert_eq!(heartbeats.last().expect("heartbeat retained").status(), &read_only);
    assert_eq!(
        heartbeats
            .observe(DaemonHeartbeat::new(fixture_id(10, HeartbeatId::new), 6, read_write.clone(),))
            .expect_err("nonce replay is rejected")
            .kind(),
        DaemonControlErrorKind::HeartbeatOrdering,
    );
    assert_eq!(
        heartbeats
            .observe(DaemonHeartbeat::new(fixture_id(11, HeartbeatId::new), 7, read_write.clone(),))
            .expect_err("heartbeat sequence cannot skip")
            .kind(),
        DaemonControlErrorKind::HeartbeatOrdering,
    );
    heartbeats
        .observe(DaemonHeartbeat::new(fixture_id(11, HeartbeatId::new), 6, read_write))
        .expect("distinct next nonce and sequence are accepted");

    let request =
        ShutdownRequest::new(fixture_id(20, RequestId::new), fixture_id(21, CorrelationId::new));
    let other_request =
        ShutdownRequest::new(fixture_id(22, RequestId::new), fixture_id(23, CorrelationId::new));
    let mut shutdown = ShutdownState::running();
    shutdown.request(request).expect("request is observed without acceptance");
    assert!(matches!(shutdown.phase(), ShutdownPhase::Requested(value) if *value == request));
    let premature =
        ShutdownComplete::new(request, ShutdownCompletionDisposition::Clean, Vec::new(), 4)
            .unwrap();
    assert_eq!(
        shutdown.complete(premature).expect_err("request alone is not completion").kind(),
        DaemonControlErrorKind::IllegalTransition,
    );
    assert_eq!(
        shutdown
            .accept(ShutdownAccepted::new(other_request))
            .expect_err("acceptance must name the retained request")
            .kind(),
        DaemonControlErrorKind::BindingMismatch,
    );
    shutdown.accept(ShutdownAccepted::new(request)).expect("exact request is explicitly accepted");
    assert!(matches!(shutdown.phase(), ShutdownPhase::Accepted(_)));

    let work =
        RemainingWork::new(RemainingWorkKind::TerminalAttachment, "terminal:active".to_owned(), 64)
            .expect("bounded remaining work");
    let progress = ShutdownProgress::new(request, 1, 3, vec![work.clone()], 4)
        .expect("bounded consistent progress");
    shutdown.progress(progress).expect("accepted shutdown begins draining");
    assert!(
        matches!(shutdown.phase(), ShutdownPhase::Draining(value) if value.completed_steps() == 1)
    );
    let regressed = ShutdownProgress::new(request, 0, 3, vec![work.clone()], 4).unwrap();
    assert_eq!(
        shutdown.progress(regressed).expect_err("progress cannot regress").kind(),
        DaemonControlErrorKind::IllegalTransition,
    );
    assert!(ShutdownComplete::new(
        request,
        ShutdownCompletionDisposition::Clean,
        vec![work.clone()],
        4,
    )
    .is_err());
    let complete =
        ShutdownComplete::new(request, ShutdownCompletionDisposition::Clean, Vec::new(), 4)
            .unwrap();
    shutdown.complete(complete.clone()).expect("clean completion is explicit");
    shutdown.complete(complete).expect("exact completion is idempotent");
    assert!(matches!(shutdown.phase(), ShutdownPhase::Completed(value)
        if value.disposition() == ShutdownCompletionDisposition::Clean && value.remaining().is_empty()));

    let mut unclean = ShutdownState::running();
    unclean.request(other_request).unwrap();
    unclean.accept(ShutdownAccepted::new(other_request)).unwrap();
    let unclean_complete =
        ShutdownComplete::new(other_request, ShutdownCompletionDisposition::Unclean, vec![work], 4)
            .expect("unclean completion truthfully retains work");
    unclean.complete(unclean_complete).unwrap();
    assert!(matches!(unclean.phase(), ShutdownPhase::Completed(value)
        if value.disposition() == ShutdownCompletionDisposition::Unclean
            && value.remaining().len() == 1));
}
