//! Cancellation, byte-stream ownership, and retry-planning tests.

#[path = "support/runtime.rs"]
mod runtime;

use std::time::Duration;

use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, HttpHeaders, HttpLimits, HttpResponse,
    MemoryByteStream, ProviderCoreError, ProviderCoreErrorKind, RetryAction, RetryFailure,
    RetryObservation, RetryPolicy, RetryProtection, StatusCode, SubmissionState, wait_for_backoff,
};

fn policy() -> RetryPolicy {
    RetryPolicy::new(
        3,
        [
            Duration::from_millis(10),
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(10),
        ],
        1_000,
    )
    .expect("retry policy")
}

#[test]
fn cancellation_is_idempotent_and_wakes_registered_waiters() {
    runtime::block_on(async {
        let token = CancellationToken::new();
        let waiter_token = token.clone();
        let waiter = tokio::spawn(async move {
            waiter_token.cancelled().await;
            waiter_token.is_cancelled()
        });
        tokio::task::yield_now().await;
        assert!(token.cancel());
        assert!(!token.cancel());
        assert!(waiter.await.expect("owned waiter completion"));
        token.cancelled().await;
    });
}

#[test]
fn retry_table_distinguishes_safe_transient_and_terminal_failures() {
    for failure in [
        RetryFailure::Connect,
        RetryFailure::Transport,
        RetryFailure::RateLimited,
        RetryFailure::Server,
    ] {
        let plan = policy()
            .plan(RetryObservation::new(1, Duration::ZERO, 0, SubmissionState::NotSent, failure))
            .expect("transient plan");
        assert_eq!(plan.action(), RetryAction::RetryFresh);
        assert_eq!(plan.delay(), Duration::from_millis(10));
    }

    for failure in [
        RetryFailure::InvalidRequest,
        RetryFailure::Authentication,
        RetryFailure::Refusal,
        RetryFailure::Malformed,
        RetryFailure::Cancelled,
    ] {
        let plan = policy()
            .plan(RetryObservation::new(1, Duration::ZERO, 0, SubmissionState::NotSent, failure))
            .expect("terminal plan");
        assert_eq!(plan.action(), RetryAction::Stop);
        assert_eq!(plan.delay(), Duration::ZERO);
    }

    let completed = policy()
        .plan(RetryObservation::new(
            1,
            Duration::ZERO,
            0,
            SubmissionState::Completed,
            RetryFailure::Completed,
        ))
        .expect("completed plan");
    assert_eq!(completed.action(), RetryAction::Stop);
}

#[test]
fn retry_table_never_blindly_replays_ambiguous_or_partial_work() {
    let ambiguous = RetryObservation::new(
        1,
        Duration::ZERO,
        10,
        SubmissionState::MaybeSent,
        RetryFailure::Transport,
    );
    assert_eq!(policy().plan(ambiguous).expect("plan").action(), RetryAction::Stop);
    assert_eq!(
        policy()
            .plan(ambiguous.with_protection(RetryProtection::IdempotencyKey))
            .expect("idempotent plan")
            .action(),
        RetryAction::RetryFresh
    );
    assert_eq!(
        policy()
            .plan(
                RetryObservation::new(
                    1,
                    Duration::ZERO,
                    10,
                    SubmissionState::AcceptedNoEvents,
                    RetryFailure::Server,
                )
                .with_protection(RetryProtection::Resume),
            )
            .expect("resume plan")
            .action(),
        RetryAction::Resume
    );
    assert_eq!(
        policy()
            .plan(
                RetryObservation::new(
                    1,
                    Duration::ZERO,
                    10,
                    SubmissionState::PartialStream,
                    RetryFailure::Transport,
                )
                .with_protection(RetryProtection::IdempotencyKey),
            )
            .expect("partial plan")
            .action(),
        RetryAction::Stop
    );
    assert_eq!(
        policy()
            .plan(
                RetryObservation::new(
                    1,
                    Duration::ZERO,
                    10,
                    SubmissionState::PartialStream,
                    RetryFailure::Transport,
                )
                .with_protection(RetryProtection::Resume),
            )
            .expect("partial resume")
            .action(),
        RetryAction::Resume
    );
}

#[test]
fn retry_table_covers_every_submission_failure_and_protection_combination() {
    let submissions = [
        SubmissionState::NotSent,
        SubmissionState::Rejected,
        SubmissionState::MaybeSent,
        SubmissionState::AcceptedNoEvents,
        SubmissionState::PartialStream,
        SubmissionState::Completed,
    ];
    let failures = [
        RetryFailure::Connect,
        RetryFailure::Transport,
        RetryFailure::RateLimited,
        RetryFailure::Server,
        RetryFailure::InvalidRequest,
        RetryFailure::Authentication,
        RetryFailure::Refusal,
        RetryFailure::Malformed,
        RetryFailure::Cancelled,
        RetryFailure::Completed,
    ];
    let protections =
        [RetryProtection::None, RetryProtection::IdempotencyKey, RetryProtection::Resume];

    for submission in submissions {
        for failure in failures {
            for protection in protections {
                let observation = RetryObservation::new(1, Duration::ZERO, 0, submission, failure)
                    .with_protection(protection);
                match expected_retry_action(submission, failure, protection) {
                    Some(expected) => assert_eq!(
                        policy().plan(observation).expect("valid table entry").action(),
                        expected,
                        "submission={submission:?} failure={failure:?} protection={protection:?}"
                    ),
                    None => assert_eq!(
                        policy().plan(observation).expect_err("contradictory table entry").kind(),
                        ProviderCoreErrorKind::InvalidRetry,
                        "submission={submission:?} failure={failure:?} protection={protection:?}"
                    ),
                }
            }
        }
    }
}

fn expected_retry_action(
    submission: SubmissionState,
    failure: RetryFailure,
    protection: RetryProtection,
) -> Option<RetryAction> {
    if (submission == SubmissionState::Completed) != (failure == RetryFailure::Completed) {
        return None;
    }
    if matches!(
        failure,
        RetryFailure::InvalidRequest
            | RetryFailure::Authentication
            | RetryFailure::Refusal
            | RetryFailure::Malformed
            | RetryFailure::Cancelled
            | RetryFailure::Completed
    ) {
        return Some(RetryAction::Stop);
    }
    Some(match submission {
        SubmissionState::NotSent | SubmissionState::Rejected => RetryAction::RetryFresh,
        SubmissionState::MaybeSent => match protection {
            RetryProtection::IdempotencyKey => RetryAction::RetryFresh,
            RetryProtection::None | RetryProtection::Resume => RetryAction::Stop,
        },
        SubmissionState::AcceptedNoEvents => match protection {
            RetryProtection::IdempotencyKey => RetryAction::RetryFresh,
            RetryProtection::Resume => RetryAction::Resume,
            RetryProtection::None => RetryAction::Stop,
        },
        SubmissionState::PartialStream => match protection {
            RetryProtection::Resume => RetryAction::Resume,
            RetryProtection::None | RetryProtection::IdempotencyKey => RetryAction::Stop,
        },
        SubmissionState::Completed => RetryAction::Stop,
    })
}

#[test]
fn retry_bounds_exhaustion_retry_after_and_jitter_are_checked() {
    let exhausted = RetryObservation::new(
        3,
        Duration::ZERO,
        0,
        SubmissionState::NotSent,
        RetryFailure::Connect,
    );
    assert_eq!(policy().plan(exhausted).expect("exhaustion").action(), RetryAction::Stop);

    let retry_after = RetryObservation::new(
        1,
        Duration::ZERO,
        0,
        SubmissionState::NotSent,
        RetryFailure::RateLimited,
    )
    .with_retry_after(Duration::from_millis(750));
    assert_eq!(
        policy().plan(retry_after).expect("retry after").delay(),
        Duration::from_millis(750)
    );

    let too_long = retry_after.with_retry_after(Duration::from_secs(2));
    assert_eq!(
        policy().plan(too_long).expect_err("retry-after bound").kind(),
        ProviderCoreErrorKind::InvalidRetry
    );
    let bad_jitter = retry_after.with_jitter_unit(10_001);
    assert_eq!(
        policy().plan(bad_jitter).expect_err("jitter bound").kind(),
        ProviderCoreErrorKind::InvalidRetry
    );
}

#[test]
fn checked_backoff_is_cancellation_aware() {
    runtime::block_on(async {
        let plan = policy()
            .plan(RetryObservation::new(
                1,
                Duration::ZERO,
                0,
                SubmissionState::NotSent,
                RetryFailure::Connect,
            ))
            .expect("plan");
        let cancellation = CancellationToken::new();
        assert!(cancellation.cancel());
        let error = wait_for_backoff(plan, &cancellation).await.expect_err("cancelled backoff");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Cancelled);
    });
}

struct OneChunk(Option<Vec<u8>>);

impl ByteStream for OneChunk {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}

#[test]
fn response_wrapper_enforces_chunk_and_cumulative_body_bounds() {
    runtime::block_on(async {
        let limits = HttpLimits::new([4, 128, 8, 8, 4]).expect("limits");
        let response = HttpResponse::new(
            StatusCode::new(200).expect("status"),
            HttpHeaders::empty(),
            Box::new(OneChunk(Some(vec![0; 5]))),
            limits,
        )
        .expect("response metadata");
        let (_, _, mut body) = response.into_parts();
        let error = body.next(&CancellationToken::new()).await.expect_err("oversized chunk");
        assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);

        assert!(MemoryByteStream::new(vec![vec![0; 4], vec![0; 5]], limits).is_err());
        let mut stream = MemoryByteStream::new(vec![b"one".to_vec(), b"two".to_vec()], limits)
            .expect("bounded memory stream");
        let cancellation = CancellationToken::new();
        assert_eq!(stream.next(&cancellation).await.expect("first"), Some(b"one".to_vec()));
        assert_eq!(stream.next(&cancellation).await.expect("second"), Some(b"two".to_vec()));
        assert_eq!(stream.next(&cancellation).await.expect("end"), None);
    });
}
