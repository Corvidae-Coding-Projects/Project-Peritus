//! Deterministic bounded retry legality and delay planning.

use crate::{ProtocolError, ProtocolErrorKind};

/// Failure/phase fact supplied to the pure planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryCause {
    /// Local failure before any request bytes.
    BeforeSend,
    /// Connection failed before request submission.
    Connect,
    /// Provider explicitly rejected with a temporary rate limit.
    RateLimited,
    /// Provider explicitly returned a transient server failure.
    TransientProvider,
    /// A normalized provider response explicitly permits a bounded fresh request.
    SafeNewRequest,
    /// Submission may have reached the provider.
    AmbiguousSubmission,
    /// Provider accepted the response but no normalized event was exposed.
    AcceptedNoEvents,
    /// One or more application events were exposed before interruption.
    PartialStream,
    /// Invalid request.
    InvalidRequest,
    /// Authentication/permission failure.
    Authentication,
    /// Model refusal or safety outcome.
    Refusal,
    /// Malformed provider content.
    Malformed,
    /// Cancellation won.
    Cancelled,
    /// Terminal completion already occurred.
    Completed,
}

/// Documented provider mechanism protecting repeated work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdempotencyGuarantee {
    /// No documented create-deduplication or exact resumption.
    None,
    /// Provider documents create-request deduplication for the sent key.
    CreateDeduplicated,
    /// Provider documents exact cursor resumption for the same response.
    ExactResume,
    /// Both fresh-request deduplication and exact resumption are documented.
    CreateAndResume,
}

/// Reason a planner refuses further work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoRetryReason {
    /// Attempt count is exhausted.
    AttemptsExhausted,
    /// Elapsed-time budget is exhausted.
    ElapsedExhausted,
    /// Caller cancellation is active.
    Cancelled,
    /// Cause is terminal/non-retryable.
    NonRetryable,
    /// Acceptance is ambiguous without documented deduplication.
    Ambiguous,
    /// Partial output lacks exact cursor resumption.
    PartialWithoutResume,
    /// Provider retry-after exceeds the allowed delay/elapsed budget.
    RetryAfterOutOfBounds,
}

/// Complete scalar planner input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryInput {
    /// Zero-based completed attempt index.
    pub attempt: u32,
    /// Maximum total attempts including the initial request.
    pub max_attempts: u32,
    /// Elapsed time so far.
    pub elapsed_millis: u64,
    /// Maximum elapsed retry horizon.
    pub max_elapsed_millis: u64,
    /// Initial backoff delay.
    pub base_delay_millis: u64,
    /// Maximum single delay.
    pub max_delay_millis: u64,
    /// Deterministic additive jitter in millionths, at most one million.
    pub jitter_millionths: u32,
    /// Provider retry-after observation.
    pub retry_after_millis: Option<u64>,
    /// Failure/phase cause.
    pub cause: RetryCause,
    /// Documented idempotency/resume guarantee.
    pub guarantee: IdempotencyGuarantee,
    /// Whether caller cancellation is active.
    pub cancelled: bool,
}

/// Checked retry action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    /// No additional request/stream action.
    Stop(NoRetryReason),
    /// Submit a fresh request after the exact delay.
    RetryNew {
        /// Exact checked backoff delay.
        delay_millis: u64,
    },
    /// Resume the same stored response at its exact cursor.
    Resume {
        /// Exact checked backoff delay.
        delay_millis: u64,
    },
}

/// Plans one legal bounded retry action.
///
/// # Errors
///
/// Rejects malformed bounds rather than silently repairing them.
pub fn plan_retry(input: RetryInput) -> Result<RetryDecision, ProtocolError> {
    validate(input)?;
    if input.cancelled {
        return Ok(RetryDecision::Stop(NoRetryReason::Cancelled));
    }
    if input.attempt.saturating_add(1) >= input.max_attempts {
        return Ok(RetryDecision::Stop(NoRetryReason::AttemptsExhausted));
    }
    if input.elapsed_millis >= input.max_elapsed_millis {
        return Ok(RetryDecision::Stop(NoRetryReason::ElapsedExhausted));
    }
    let action = match input.cause {
        RetryCause::BeforeSend
        | RetryCause::Connect
        | RetryCause::RateLimited
        | RetryCause::TransientProvider
        | RetryCause::SafeNewRequest => Action::New,
        RetryCause::AmbiguousSubmission | RetryCause::AcceptedNoEvents => {
            if matches!(
                input.guarantee,
                IdempotencyGuarantee::CreateDeduplicated | IdempotencyGuarantee::CreateAndResume
            ) {
                Action::New
            } else if matches!(
                input.guarantee,
                IdempotencyGuarantee::ExactResume | IdempotencyGuarantee::CreateAndResume
            ) {
                Action::Resume
            } else {
                return Ok(RetryDecision::Stop(NoRetryReason::Ambiguous));
            }
        }
        RetryCause::PartialStream => {
            if matches!(
                input.guarantee,
                IdempotencyGuarantee::ExactResume | IdempotencyGuarantee::CreateAndResume
            ) {
                Action::Resume
            } else {
                return Ok(RetryDecision::Stop(NoRetryReason::PartialWithoutResume));
            }
        }
        RetryCause::InvalidRequest
        | RetryCause::Authentication
        | RetryCause::Refusal
        | RetryCause::Malformed
        | RetryCause::Cancelled
        | RetryCause::Completed => {
            return Ok(RetryDecision::Stop(NoRetryReason::NonRetryable));
        }
    };
    if input.retry_after_millis.is_some_and(|value| value > input.max_delay_millis) {
        return Ok(RetryDecision::Stop(NoRetryReason::RetryAfterOutOfBounds));
    }
    let delay = delay(input);
    if input.elapsed_millis.saturating_add(delay) > input.max_elapsed_millis {
        return Ok(RetryDecision::Stop(NoRetryReason::ElapsedExhausted));
    }
    Ok(match action {
        Action::New => RetryDecision::RetryNew { delay_millis: delay },
        Action::Resume => RetryDecision::Resume { delay_millis: delay },
    })
}

#[derive(Clone, Copy)]
enum Action {
    New,
    Resume,
}

fn validate(input: RetryInput) -> Result<(), ProtocolError> {
    if input.max_attempts == 0
        || input.max_elapsed_millis == 0
        || input.base_delay_millis == 0
        || input.max_delay_millis < input.base_delay_millis
        || input.jitter_millionths > 1_000_000
    {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidRetry,
            "retry",
            "retry bounds are zero, inverted, or outside the jitter range",
        ));
    }
    Ok(())
}

fn delay(input: RetryInput) -> u64 {
    let shift = input.attempt.min(63);
    let exponential =
        input.base_delay_millis.checked_shl(shift).unwrap_or(u64::MAX).min(input.max_delay_millis);
    let jitter = exponential
        .saturating_mul(u64::from(input.jitter_millionths))
        .checked_div(1_000_000)
        .unwrap_or(0);
    let computed = exponential.saturating_add(jitter).min(input.max_delay_millis);
    input.retry_after_millis.map_or(computed, |provider| provider.max(computed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(cause: RetryCause, guarantee: IdempotencyGuarantee) -> RetryInput {
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
    fn ambiguous_requires_documented_guarantee() {
        assert_eq!(
            plan_retry(input(RetryCause::AmbiguousSubmission, IdempotencyGuarantee::None))
                .expect("valid input"),
            RetryDecision::Stop(NoRetryReason::Ambiguous)
        );
        assert!(matches!(
            plan_retry(input(
                RetryCause::AmbiguousSubmission,
                IdempotencyGuarantee::CreateDeduplicated
            ))
            .expect("valid input"),
            RetryDecision::RetryNew { .. }
        ));
    }

    #[test]
    fn partial_stream_requires_exact_resume() {
        assert_eq!(
            plan_retry(input(RetryCause::PartialStream, IdempotencyGuarantee::None))
                .expect("valid input"),
            RetryDecision::Stop(NoRetryReason::PartialWithoutResume)
        );
        assert!(matches!(
            plan_retry(input(RetryCause::PartialStream, IdempotencyGuarantee::ExactResume))
                .expect("valid input"),
            RetryDecision::Resume { .. }
        ));
    }
}
