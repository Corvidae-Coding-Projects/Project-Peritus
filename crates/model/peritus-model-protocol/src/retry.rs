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
    pub max_elapsed_millis: Option<u64>,
    /// Initial backoff delay.
    pub base_delay_millis: u64,
    /// Maximum allowed delay for this finite retry budget.
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

/// Retry input with no synthetic attempt-count horizon.
///
/// This form is for work that is known not to have been accepted. It retains the same checked
/// backoff, retry-after, elapsed-time, cancellation, and retry-safety policy as [`RetryInput`],
/// while allowing the owning logical task to remain recoverable for the duration selected by its
/// caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnboundedRetryInput {
    /// Zero-based completed attempt index, used only to select bounded backoff.
    pub attempt: u64,
    /// Elapsed time so far.
    pub elapsed_millis: u64,
    /// Maximum elapsed retry horizon.
    pub max_elapsed_millis: Option<u64>,
    /// Initial backoff delay.
    pub base_delay_millis: u64,
    /// Maximum host-selected backoff. A provider-required retry-after may exceed this value.
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
    validate_finite(input)?;
    Ok(plan(
        RetryFacts::from(input),
        AttemptPolicy::Finite { max_attempts: u64::from(input.max_attempts) },
        RequiredDelayPolicy::RejectAboveLocalMaximum,
    ))
}

/// Plans a legal retry without treating completed attempts as a work budget.
///
/// Cancellation, a configured elapsed horizon, retry-after bounds, and acceptance safety remain
/// authoritative. The attempt number affects exponential backoff only; delay calculation
/// saturates at the configured per-wait maximum.
///
/// # Errors
///
/// Rejects malformed bounds rather than silently repairing them.
pub fn plan_unbounded_retry(
    input: UnboundedRetryInput,
) -> Result<RetryDecision, ProtocolError> {
    validate_unbounded(input)?;
    Ok(plan(
        RetryFacts::from(input),
        AttemptPolicy::Unbounded,
        RequiredDelayPolicy::HonorProviderMinimum,
    ))
}

fn plan(
    input: RetryFacts,
    attempt_policy: AttemptPolicy,
    required_delay_policy: RequiredDelayPolicy,
) -> RetryDecision {
    if input.cancelled {
        return RetryDecision::Stop(NoRetryReason::Cancelled);
    }
    if let AttemptPolicy::Finite { max_attempts } = attempt_policy {
        if input.attempt.saturating_add(1) >= max_attempts {
            return RetryDecision::Stop(NoRetryReason::AttemptsExhausted);
        }
    }
    if input.max_elapsed_millis.is_some_and(|maximum| input.elapsed_millis >= maximum) {
        return RetryDecision::Stop(NoRetryReason::ElapsedExhausted);
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
                return RetryDecision::Stop(NoRetryReason::Ambiguous);
            }
        }
        RetryCause::PartialStream => {
            if matches!(
                input.guarantee,
                IdempotencyGuarantee::ExactResume | IdempotencyGuarantee::CreateAndResume
            ) {
                Action::Resume
            } else {
                return RetryDecision::Stop(NoRetryReason::PartialWithoutResume);
            }
        }
        RetryCause::InvalidRequest
        | RetryCause::Authentication
        | RetryCause::Refusal
        | RetryCause::Malformed
        | RetryCause::Cancelled
        | RetryCause::Completed => {
            return RetryDecision::Stop(NoRetryReason::NonRetryable);
        }
    };
    if required_delay_policy == RequiredDelayPolicy::RejectAboveLocalMaximum
        && input.retry_after_millis.is_some_and(|value| value > input.max_delay_millis)
    {
        return RetryDecision::Stop(NoRetryReason::RetryAfterOutOfBounds);
    }
    let delay = delay(input);
    if input
        .max_elapsed_millis
        .is_some_and(|maximum| input.elapsed_millis.saturating_add(delay) > maximum)
    {
        return RetryDecision::Stop(NoRetryReason::ElapsedExhausted);
    }
    match action {
        Action::New => RetryDecision::RetryNew { delay_millis: delay },
        Action::Resume => RetryDecision::Resume { delay_millis: delay },
    }
}

#[derive(Clone, Copy)]
enum Action {
    New,
    Resume,
}

#[derive(Clone, Copy)]
enum AttemptPolicy {
    Finite { max_attempts: u64 },
    Unbounded,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RequiredDelayPolicy {
    RejectAboveLocalMaximum,
    HonorProviderMinimum,
}

#[derive(Clone, Copy)]
struct RetryFacts {
    attempt: u64,
    elapsed_millis: u64,
    max_elapsed_millis: Option<u64>,
    base_delay_millis: u64,
    max_delay_millis: u64,
    jitter_millionths: u32,
    retry_after_millis: Option<u64>,
    cause: RetryCause,
    guarantee: IdempotencyGuarantee,
    cancelled: bool,
}

impl From<RetryInput> for RetryFacts {
    fn from(input: RetryInput) -> Self {
        Self {
            attempt: u64::from(input.attempt),
            elapsed_millis: input.elapsed_millis,
            max_elapsed_millis: input.max_elapsed_millis,
            base_delay_millis: input.base_delay_millis,
            max_delay_millis: input.max_delay_millis,
            jitter_millionths: input.jitter_millionths,
            retry_after_millis: input.retry_after_millis,
            cause: input.cause,
            guarantee: input.guarantee,
            cancelled: input.cancelled,
        }
    }
}

impl From<UnboundedRetryInput> for RetryFacts {
    fn from(input: UnboundedRetryInput) -> Self {
        Self {
            attempt: input.attempt,
            elapsed_millis: input.elapsed_millis,
            max_elapsed_millis: input.max_elapsed_millis,
            base_delay_millis: input.base_delay_millis,
            max_delay_millis: input.max_delay_millis,
            jitter_millionths: input.jitter_millionths,
            retry_after_millis: input.retry_after_millis,
            cause: input.cause,
            guarantee: input.guarantee,
            cancelled: input.cancelled,
        }
    }
}

fn validate_finite(input: RetryInput) -> Result<(), ProtocolError> {
    if input.max_attempts == 0 {
        return Err(invalid_retry_bounds());
    }
    validate_facts(RetryFacts::from(input))
}

fn validate_unbounded(input: UnboundedRetryInput) -> Result<(), ProtocolError> {
    validate_facts(RetryFacts::from(input))
}

fn validate_facts(input: RetryFacts) -> Result<(), ProtocolError> {
    if input.max_elapsed_millis == Some(0)
        || input.base_delay_millis == 0
        || input.max_delay_millis < input.base_delay_millis
        || input.jitter_millionths > 1_000_000
    {
        return Err(invalid_retry_bounds());
    }
    Ok(())
}

fn invalid_retry_bounds() -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidRetry,
        "retry",
        "retry bounds are zero, inverted, or outside the jitter range",
    )
}

fn delay(input: RetryFacts) -> u64 {
    let shift = input.attempt.min(63);
    let exponential =
        input.base_delay_millis
            .checked_mul(1_u64 << shift)
            .unwrap_or(u64::MAX)
            .min(input.max_delay_millis);
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
            max_elapsed_millis: Some(10_000),
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
    fn absent_elapsed_bound_preserves_legal_retry_after_a_long_model_call() {
        let mut request = input(RetryCause::SafeNewRequest, IdempotencyGuarantee::None);
        request.max_elapsed_millis = None;
        request.elapsed_millis = 12 * 60 * 60 * 1000;
        assert!(matches!(plan_retry(request).expect("retry"), RetryDecision::RetryNew { .. }));
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
