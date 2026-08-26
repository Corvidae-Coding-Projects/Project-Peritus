//! Resumable at-least-once subscription trace integration tests.

mod support;

use peritus_app_protocol::{
    Acknowledgement, AppProtocolLimits, CancellationDisposition, CorrelationId, DeliveryAdmission,
    DeliveryAttemptId, EventCursor, PauseReason, RegisteredEventFrame, SubscriptionCancellation,
    SubscriptionCancellationSource, SubscriptionErrorKind, SubscriptionFilter, SubscriptionGap,
    SubscriptionId, SubscriptionPhase, SubscriptionState,
};
use peritus_types::EventId;
use support::{event_fixture_bytes, fixture_id};

fn event_frame() -> RegisteredEventFrame {
    RegisteredEventFrame::new(event_fixture_bytes(), AppProtocolLimits::PRODUCTION.codec())
        .expect("registered B3 event fixture")
}

#[allow(
    clippy::too_many_lines,
    reason = "one sequential trace keeps delivery, acknowledgement, gap, and cancellation causality visible"
)]
#[test]
fn resume_redelivery_ack_gap_and_backpressure_are_explicit() {
    let subscription_id = fixture_id(10, SubscriptionId::new);
    let filter = SubscriptionFilter::new(vec!["agent.events".to_owned()], 4, 64)
        .expect("bounded canonical filter");
    let mut state = SubscriptionState::new(subscription_id, filter.clone(), EventCursor::new(7), 2)
        .expect("positive delivery window");

    let first = match state
        .deliver(
            fixture_id(11, EventId::new),
            fixture_id(12, DeliveryAttemptId::new),
            event_frame(),
        )
        .expect("first delivery")
    {
        DeliveryAdmission::Delivered(delivery) => delivery,
        DeliveryAdmission::Backpressured => panic!("empty window cannot be backpressured"),
    };
    assert_eq!(first.cursor(), EventCursor::new(8));
    assert_eq!(first.attempt(), 1);

    let redelivery = state
        .redeliver(first.cursor(), fixture_id(13, DeliveryAttemptId::new))
        .expect("unacknowledged delivery may be retried");
    assert_eq!(redelivery.event_id(), first.event_id());
    assert_eq!(redelivery.cursor(), first.cursor());
    assert_eq!(redelivery.frame().bytes(), first.frame().bytes());
    assert_eq!(redelivery.frame().digest(), first.frame().digest());
    assert_ne!(redelivery.attempt_id(), first.attempt_id());
    assert_eq!(redelivery.attempt(), 2);

    let second = state
        .deliver(
            fixture_id(14, EventId::new),
            fixture_id(15, DeliveryAttemptId::new),
            event_frame(),
        )
        .expect("second delivery");
    assert!(
        matches!(second, DeliveryAdmission::Delivered(ref value) if value.cursor() == EventCursor::new(9))
    );
    assert_eq!(
        state
            .deliver(
                fixture_id(16, EventId::new),
                fixture_id(17, DeliveryAttemptId::new),
                event_frame(),
            )
            .expect("capacity is an explicit admission result"),
        DeliveryAdmission::Backpressured,
    );

    let wrong_ack = state
        .acknowledge(Acknowledgement::new(fixture_id(18, SubscriptionId::new), EventCursor::new(8)))
        .expect_err("acknowledgement is subscription-scoped");
    assert_eq!(wrong_ack.kind(), SubscriptionErrorKind::BindingMismatch);
    assert_eq!(
        state
            .acknowledge(Acknowledgement::new(subscription_id, EventCursor::new(8)))
            .expect("cumulative acknowledgement releases first prefix"),
        1,
    );
    assert_eq!(
        state
            .acknowledge(Acknowledgement::new(subscription_id, EventCursor::new(8)))
            .expect("repeating current cumulative acknowledgement is legal"),
        0,
    );
    assert_eq!(
        state
            .acknowledge(Acknowledgement::new(subscription_id, EventCursor::new(7)))
            .expect_err("acknowledgement regression is rejected")
            .kind(),
        SubscriptionErrorKind::AcknowledgementRegression,
    );
    assert_eq!(
        state
            .acknowledge(Acknowledgement::new(subscription_id, EventCursor::new(10)))
            .expect_err("future acknowledgement is rejected")
            .kind(),
        SubscriptionErrorKind::AcknowledgementFuture,
    );
    assert_eq!(
        state
            .acknowledge(Acknowledgement::new(subscription_id, EventCursor::new(9)))
            .expect("remaining prefix is released"),
        1,
    );

    state.pause(PauseReason::SlowConsumer).expect("slow consumer pause");
    assert!(matches!(state.phase(), SubscriptionPhase::Paused(PauseReason::SlowConsumer)));
    assert_eq!(
        state
            .deliver(
                fixture_id(19, EventId::new),
                fixture_id(20, DeliveryAttemptId::new),
                event_frame(),
            )
            .expect_err("paused delivery is rejected")
            .kind(),
        SubscriptionErrorKind::IllegalTransition,
    );
    state.resume().expect("paused subscription resumes");

    let gap_id = fixture_id(21, SubscriptionId::new);
    let mut gap_state = SubscriptionState::new(gap_id, filter, EventCursor::new(3), 2)
        .expect("resume subscription");
    let gap = SubscriptionGap::new(EventCursor::new(3), EventCursor::new(5), EventCursor::new(12))
        .expect("request predates retained interval");
    gap_state.declare_gap(gap).expect("retention gap is explicit");
    assert!(
        matches!(gap_state.phase(), SubscriptionPhase::SnapshotRequired(value) if value == gap)
    );
    assert_eq!(
        gap_state
            .acknowledge(Acknowledgement::new(gap_id, EventCursor::new(5)))
            .expect_err("acknowledgement cannot cross a gap")
            .kind(),
        SubscriptionErrorKind::AcknowledgementAcrossGap,
    );

    let cancellation = SubscriptionCancellation::new(
        gap_id,
        fixture_id(22, CorrelationId::new),
        SubscriptionCancellationSource::Client,
    );
    assert_eq!(
        gap_state.cancel(cancellation).expect("gap subscription cancels"),
        CancellationDisposition::Applied,
    );
    assert_eq!(
        gap_state.cancel(cancellation).expect("same cancellation is idempotent"),
        CancellationDisposition::Repeated,
    );
    assert_eq!(
        gap_state
            .deliver(
                fixture_id(23, EventId::new),
                fixture_id(24, DeliveryAttemptId::new),
                event_frame(),
            )
            .expect_err("cancelled subscription never delivers again")
            .kind(),
        SubscriptionErrorKind::IllegalTransition,
    );
}
