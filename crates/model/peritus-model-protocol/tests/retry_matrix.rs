//! Exhaustive provider-neutral retry-cause and guarantee matrix.

use peritus_model_protocol::{
    IdempotencyGuarantee, NoRetryReason, ProtocolErrorKind, RetryCause, RetryDecision, RetryInput,
    plan_retry,
};

const CAUSES: [RetryCause; 13] = [
    RetryCause::BeforeSend,
    RetryCause::Connect,
    RetryCause::RateLimited,
    RetryCause::TransientProvider,
    RetryCause::AmbiguousSubmission,
    RetryCause::AcceptedNoEvents,
    RetryCause::PartialStream,
    RetryCause::InvalidRequest,
    RetryCause::Authentication,
    RetryCause::Refusal,
    RetryCause::Malformed,
    RetryCause::Cancelled,
    RetryCause::Completed,
];

const GUARANTEES: [IdempotencyGuarantee; 4] = [
    IdempotencyGuarantee::None,
    IdempotencyGuarantee::CreateDeduplicated,
    IdempotencyGuarantee::ExactResume,
    IdempotencyGuarantee::CreateAndResume,
];

const fn input(cause: RetryCause, guarantee: IdempotencyGuarantee) -> RetryInput {
    RetryInput {
        attempt: 0,
        max_attempts: 4,
        elapsed_millis: 0,
        max_elapsed_millis: 10_000,
        base_delay_millis: 100,
        max_delay_millis: 2_000,
        jitter_millionths: 0,
        retry_after_millis: None,
        cause,
        guarantee,
        cancelled: false,
    }
}

#[test]
fn every_retry_cause_and_provider_guarantee_has_one_exact_decision() {
    for cause in CAUSES {
        for guarantee in GUARANTEES {
            assert_eq!(
                plan_retry(input(cause, guarantee)).expect("valid table input"),
                expected(cause, guarantee),
                "cause={cause:?} guarantee={guarantee:?}"
            );
        }
    }
}

const fn expected(cause: RetryCause, guarantee: IdempotencyGuarantee) -> RetryDecision {
    match cause {
        RetryCause::BeforeSend
        | RetryCause::Connect
        | RetryCause::RateLimited
        | RetryCause::TransientProvider => RetryDecision::RetryNew { delay_millis: 100 },
        RetryCause::AmbiguousSubmission | RetryCause::AcceptedNoEvents => match guarantee {
            IdempotencyGuarantee::CreateDeduplicated | IdempotencyGuarantee::CreateAndResume => {
                RetryDecision::RetryNew { delay_millis: 100 }
            }
            IdempotencyGuarantee::ExactResume => RetryDecision::Resume { delay_millis: 100 },
            IdempotencyGuarantee::None => RetryDecision::Stop(NoRetryReason::Ambiguous),
        },
        RetryCause::PartialStream => match guarantee {
            IdempotencyGuarantee::ExactResume | IdempotencyGuarantee::CreateAndResume => {
                RetryDecision::Resume { delay_millis: 100 }
            }
            IdempotencyGuarantee::None | IdempotencyGuarantee::CreateDeduplicated => {
                RetryDecision::Stop(NoRetryReason::PartialWithoutResume)
            }
        },
        RetryCause::InvalidRequest
        | RetryCause::Authentication
        | RetryCause::Refusal
        | RetryCause::Malformed
        | RetryCause::Cancelled
        | RetryCause::Completed => RetryDecision::Stop(NoRetryReason::NonRetryable),
    }
}

#[test]
fn retry_bounds_cancellation_retry_after_and_elapsed_horizon_fail_closed() {
    let base = input(RetryCause::RateLimited, IdempotencyGuarantee::None);
    assert_eq!(
        plan_retry(RetryInput { cancelled: true, ..base }).expect("cancelled input"),
        RetryDecision::Stop(NoRetryReason::Cancelled)
    );
    assert_eq!(
        plan_retry(RetryInput { attempt: 3, ..base }).expect("exhausted input"),
        RetryDecision::Stop(NoRetryReason::AttemptsExhausted)
    );
    assert_eq!(
        plan_retry(RetryInput { elapsed_millis: 10_000, ..base }).expect("elapsed input"),
        RetryDecision::Stop(NoRetryReason::ElapsedExhausted)
    );
    assert_eq!(
        plan_retry(RetryInput { retry_after_millis: Some(2_001), ..base })
            .expect("bounded planner input"),
        RetryDecision::Stop(NoRetryReason::RetryAfterOutOfBounds)
    );
    assert_eq!(
        plan_retry(RetryInput { elapsed_millis: 9_950, ..base }).expect("horizon input"),
        RetryDecision::Stop(NoRetryReason::ElapsedExhausted)
    );
    assert_eq!(
        plan_retry(RetryInput { jitter_millionths: 1_000_001, ..base })
            .expect_err("invalid jitter")
            .kind(),
        ProtocolErrorKind::InvalidRetry
    );
}
