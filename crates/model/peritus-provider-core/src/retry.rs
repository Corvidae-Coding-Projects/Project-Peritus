//! Deterministic bounded retry legality and delay planning.

use std::time::Duration;

use peritus_model_protocol::RetryLegalityFacts;

use crate::{ProviderCoreError, ProviderCoreErrorKind};

const MAX_ATTEMPTS: u32 = 16;
const MAX_DELAY: Duration = Duration::from_hours(24);
const MAX_ELAPSED: Duration = Duration::from_hours(168);
const MAX_CUMULATIVE_BYTES: u64 = 1024 * 1024 * 1024;

/// Failure class observed by retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryFailure {
    /// Connection or TLS establishment failed.
    Connect,
    /// Other transport failure or disconnect.
    Transport,
    /// Provider rate limit.
    RateLimited,
    /// Transient provider server failure.
    Server,
    /// Provider rejected request syntax or semantics.
    InvalidRequest,
    /// Provider rejected authentication or authorization.
    Authentication,
    /// Provider refused the requested model output.
    Refusal,
    /// Provider content or framing was malformed.
    Malformed,
    /// Caller cancellation.
    Cancelled,
    /// Terminal completion was already observed.
    Completed,
}

/// How far one submission progressed before failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmissionState {
    /// No request bytes could have reached the provider.
    NotSent,
    /// The provider returned an explicit non-accepting HTTP response.
    Rejected,
    /// Request bytes may have reached the provider, but acceptance is unknown.
    MaybeSent,
    /// The provider accepted the request but no normalized events were observed.
    AcceptedNoEvents,
    /// At least one normalized provider event was observed.
    PartialStream,
    /// A valid terminal event was observed.
    Completed,
}

/// Protection available for a repeated or resumed request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryProtection {
    /// Neither exact idempotency nor exact resumption is documented.
    None,
    /// Provider-documented idempotency protects exact request recreation.
    IdempotencyKey,
    /// Provider-documented cursor or response identity permits exact resumption.
    Resume,
}

/// Checked retry policy bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    max_retry_after: Duration,
    max_elapsed: Duration,
    max_cumulative_bytes: u64,
}

impl RetryPolicy {
    /// Creates a bounded retry policy.
    ///
    /// # Errors
    ///
    /// Rejects zero or production-widening limits and inconsistent delay bounds.
    pub fn new(
        max_attempts: u32,
        delays: [Duration; 4],
        max_cumulative_bytes: u64,
    ) -> Result<Self, ProviderCoreError> {
        let [base_delay, max_delay, max_retry_after, max_elapsed] = delays;
        if max_attempts == 0
            || max_attempts > MAX_ATTEMPTS
            || base_delay.is_zero()
            || base_delay > max_delay
            || max_delay > MAX_DELAY
            || max_retry_after > max_delay
            || max_elapsed.is_zero()
            || max_elapsed > MAX_ELAPSED
            || max_cumulative_bytes == 0
            || max_cumulative_bytes > MAX_CUMULATIVE_BYTES
        {
            return Err(retry_error(
                "retry policy is zero, inconsistent, or exceeds production bounds",
            ));
        }
        Ok(Self {
            max_attempts,
            base_delay,
            max_delay,
            max_retry_after,
            max_elapsed,
            max_cumulative_bytes,
        })
    }

    /// Plans the next action from checked deterministic observations.
    ///
    /// # Errors
    ///
    /// Rejects an invalid attempt, jitter, retry-after value, elapsed time, or cumulative byte
    /// count rather than silently clamping untrusted observations.
    pub fn plan(&self, observation: RetryObservation) -> Result<RetryPlan, ProviderCoreError> {
        observation.validate(*self)?;
        let action =
            legal_action(observation.submission, observation.failure, observation.protection);
        if action == RetryAction::Stop {
            return Ok(RetryPlan { action, delay: Duration::ZERO });
        }
        if observation.attempt >= self.max_attempts
            || observation.elapsed >= self.max_elapsed
            || observation.cumulative_bytes >= self.max_cumulative_bytes
        {
            return Ok(RetryPlan { action: RetryAction::Stop, delay: Duration::ZERO });
        }
        let delay = self.delay(observation.attempt, observation.jitter_unit)?;
        let delay = observation.retry_after.map_or(delay, |retry_after| delay.max(retry_after));
        if observation.elapsed.saturating_add(delay) > self.max_elapsed {
            return Ok(RetryPlan { action: RetryAction::Stop, delay: Duration::ZERO });
        }
        if !formally_legal_retry(*self, observation, action, delay) {
            return Err(retry_error("retry plan contradicted its verified legality projection"));
        }
        Ok(RetryPlan { action, delay })
    }

    fn delay(&self, attempt: u32, jitter_unit: u16) -> Result<Duration, ProviderCoreError> {
        let exponent = attempt.saturating_sub(1).min(63);
        let multiplier = 1_u128 << exponent;
        let capped_nanos =
            self.base_delay.as_nanos().saturating_mul(multiplier).min(self.max_delay.as_nanos());
        // Deterministic factor in the inclusive range 0.5 through 1.5.
        let jitter_factor = u128::from(5_000_u16 + jitter_unit);
        let jittered_nanos = capped_nanos
            .saturating_mul(jitter_factor)
            .checked_div(10_000)
            .unwrap_or(capped_nanos)
            .min(self.max_delay.as_nanos());
        let nanos = u64::try_from(jittered_nanos)
            .map_err(|_| retry_error("retry delay cannot be represented"))?;
        Ok(Duration::from_nanos(nanos))
    }
}

fn formally_legal_retry(
    policy: RetryPolicy,
    observation: RetryObservation,
    action: RetryAction,
    delay: Duration,
) -> bool {
    let fresh_retry_safe = action != RetryAction::RetryFresh
        || matches!(observation.submission, SubmissionState::NotSent | SubmissionState::Rejected)
        || observation.protection == RetryProtection::IdempotencyKey;
    let partial_has_exact_resume = action != RetryAction::Resume
        || (observation.protection == RetryProtection::Resume
            && matches!(
                observation.submission,
                SubmissionState::AcceptedNoEvents | SubmissionState::PartialStream
            ));
    peritus_model_protocol::retry_legality_complete(RetryLegalityFacts {
        bounds_allow: observation.attempt < policy.max_attempts
            && observation.cumulative_bytes < policy.max_cumulative_bytes
            && observation.elapsed.saturating_add(delay) <= policy.max_elapsed,
        not_cancelled: observation.failure != RetryFailure::Cancelled,
        not_terminal: observation.failure != RetryFailure::Completed
            && observation.submission != SubmissionState::Completed,
        fresh_retry_safe,
        partial_has_exact_resume,
        action_matches_cause: legal_action(
            observation.submission,
            observation.failure,
            observation.protection,
        ) == action,
    })
}

/// Deterministic observations supplied to retry planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryObservation {
    attempt: u32,
    elapsed: Duration,
    cumulative_bytes: u64,
    submission: SubmissionState,
    failure: RetryFailure,
    protection: RetryProtection,
    retry_after: Option<Duration>,
    jitter_unit: u16,
}

impl RetryObservation {
    /// Creates an observation with no protection, no retry-after value, and midpoint jitter.
    #[must_use]
    pub const fn new(
        attempt: u32,
        elapsed: Duration,
        cumulative_bytes: u64,
        submission: SubmissionState,
        failure: RetryFailure,
    ) -> Self {
        Self {
            attempt,
            elapsed,
            cumulative_bytes,
            submission,
            failure,
            protection: RetryProtection::None,
            retry_after: None,
            jitter_unit: 5_000,
        }
    }

    /// Adds documented idempotency or resumption protection.
    #[must_use]
    pub const fn with_protection(mut self, protection: RetryProtection) -> Self {
        self.protection = protection;
        self
    }

    /// Adds a provider retry-after observation.
    #[must_use]
    pub const fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }

    /// Adds deterministic jitter in the inclusive range 0 through 10,000.
    #[must_use]
    pub const fn with_jitter_unit(mut self, jitter_unit: u16) -> Self {
        self.jitter_unit = jitter_unit;
        self
    }

    fn validate(self, policy: RetryPolicy) -> Result<(), ProviderCoreError> {
        if self.attempt == 0
            || self.attempt > policy.max_attempts
            || self.elapsed > policy.max_elapsed
            || self.cumulative_bytes > policy.max_cumulative_bytes
            || self.jitter_unit > 10_000
            || self.retry_after.is_some_and(|delay| delay > policy.max_retry_after)
        {
            return Err(retry_error("retry observation is outside policy bounds"));
        }
        if (self.submission == SubmissionState::Completed)
            != (self.failure == RetryFailure::Completed)
        {
            return Err(retry_error("terminal submission and failure observations disagree"));
        }
        Ok(())
    }
}

/// Executable retry action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryAction {
    /// Do not retry.
    Stop,
    /// Recreate the exact request as a new HTTP submission.
    RetryFresh,
    /// Resume through a provider-documented exact response cursor.
    Resume,
}

/// Checked action and bounded delay returned by retry planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPlan {
    action: RetryAction,
    delay: Duration,
}

impl RetryPlan {
    /// Returns the action.
    #[must_use]
    pub const fn action(self) -> RetryAction {
        self.action
    }

    /// Returns the cancellation-aware backoff duration to execute first.
    #[must_use]
    pub const fn delay(self) -> Duration {
        self.delay
    }
}

fn legal_action(
    submission: SubmissionState,
    failure: RetryFailure,
    protection: RetryProtection,
) -> RetryAction {
    if matches!(
        failure,
        RetryFailure::InvalidRequest
            | RetryFailure::Authentication
            | RetryFailure::Refusal
            | RetryFailure::Malformed
            | RetryFailure::Cancelled
            | RetryFailure::Completed
    ) {
        return RetryAction::Stop;
    }
    match submission {
        SubmissionState::NotSent | SubmissionState::Rejected => RetryAction::RetryFresh,
        SubmissionState::MaybeSent | SubmissionState::AcceptedNoEvents => match protection {
            RetryProtection::IdempotencyKey => RetryAction::RetryFresh,
            RetryProtection::Resume if submission == SubmissionState::AcceptedNoEvents => {
                RetryAction::Resume
            }
            RetryProtection::None | RetryProtection::Resume => RetryAction::Stop,
        },
        SubmissionState::PartialStream => match protection {
            RetryProtection::Resume => RetryAction::Resume,
            RetryProtection::None | RetryProtection::IdempotencyKey => RetryAction::Stop,
        },
        SubmissionState::Completed => RetryAction::Stop,
    }
}

const fn retry_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::InvalidRetry, "retry", detail)
}
