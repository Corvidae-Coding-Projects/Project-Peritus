//! Checked retry planning and cancellation-aware backoff for developer model turns.

use std::time::{Duration, Instant};

use peritus_model_protocol::{
    IdempotencyGuarantee, RetryCause, RetryDecision, RetryInput, Retryability, TerminalOutcome,
    plan_retry,
};
use peritus_provider_core::{CancellationToken, ProviderCoreErrorKind, cancel_first};

use crate::ModelDriveError;

use super::{
    DeveloperLoopError, DeveloperRetryReason, DeveloperRetryRecord, DeveloperTrace,
    DeveloperTraceEvent,
};

const BASE_DELAY_MILLIS: u64 = 250;
const MAX_DELAY_MILLIS: u64 = 30_000;
const MAX_ELAPSED_MILLIS: u64 = 120_000;
const MAX_JITTER_MILLIONTHS: u32 = 250_000;

/// One logical turn's retry horizon and stable jitter identity.
pub(super) struct DeveloperRetryPlanner<'a> {
    request_prefix: &'a str,
    turn: u16,
    maximum: u8,
    started: Instant,
    cancellation: &'a CancellationToken,
}

impl<'a> DeveloperRetryPlanner<'a> {
    pub(super) fn new(
        request_prefix: &'a str,
        turn: u16,
        maximum: u8,
        cancellation: &'a CancellationToken,
    ) -> Self {
        Self { request_prefix, turn, maximum, started: Instant::now(), cancellation }
    }

    pub(super) fn terminal(
        &self,
        attempt: u8,
        terminal: Option<&TerminalOutcome>,
        usable: bool,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        if self.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        match terminal {
            Some(TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. })
                if !usable =>
            {
                self.plan(
                    attempt,
                    RetryCause::SafeNewRequest,
                    None,
                    DeveloperRetryReason::EmptyResponse,
                )
            }
            Some(TerminalOutcome::Failed(failure))
                if matches!(
                    failure.retryability(),
                    Retryability::SafeNewRequest | Retryability::CallerDecision
                ) =>
            {
                self.plan(
                    attempt,
                    RetryCause::SafeNewRequest,
                    failure.retry_after_millis(),
                    DeveloperRetryReason::RetryableProviderResponse,
                )
            }
            None if !usable => self.plan(
                attempt,
                RetryCause::SafeNewRequest,
                None,
                DeveloperRetryReason::EmptyResponse,
            ),
            Some(_) | None => Ok(None),
        }
    }

    pub(super) fn error(
        &self,
        attempt: u8,
        error: &DeveloperLoopError,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        if self.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        let (cause, reason) = match error {
            DeveloperLoopError::Model(ModelDriveError::Provider(provider)) => {
                match provider.kind() {
                    ProviderCoreErrorKind::Connect => {
                        (RetryCause::Connect, DeveloperRetryReason::Connection)
                    }
                    ProviderCoreErrorKind::Transport => {
                        (RetryCause::SafeNewRequest, DeveloperRetryReason::Transport)
                    }
                    ProviderCoreErrorKind::MalformedStream => {
                        (RetryCause::SafeNewRequest, DeveloperRetryReason::MalformedStream)
                    }
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        self.plan(attempt, cause, None, reason)
    }

    pub(super) async fn record_and_wait(
        &self,
        record: &DeveloperRetryRecord,
        trace: &mut dyn DeveloperTrace,
    ) -> Result<(), DeveloperLoopError> {
        trace.record(DeveloperTraceEvent::RetryScheduled(record))?;
        match cancel_first(
            self.cancellation,
            tokio::time::sleep(Duration::from_millis(record.delay_millis())),
        )
        .await
        {
            None => Err(DeveloperLoopError::Cancelled),
            Some(()) => Ok(()),
        }
    }

    fn plan(
        &self,
        attempt: u8,
        cause: RetryCause,
        retry_after_millis: Option<u64>,
        reason: DeveloperRetryReason,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        let elapsed_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let input = RetryInput {
            attempt: u32::from(attempt.saturating_sub(1)),
            max_attempts: u32::from(self.maximum),
            elapsed_millis,
            max_elapsed_millis: MAX_ELAPSED_MILLIS,
            base_delay_millis: BASE_DELAY_MILLIS,
            max_delay_millis: MAX_DELAY_MILLIS,
            jitter_millionths: deterministic_jitter(self.request_prefix, self.turn, attempt),
            retry_after_millis,
            cause,
            guarantee: IdempotencyGuarantee::None,
            cancelled: self.cancellation.is_cancelled(),
        };
        Ok(match plan_retry(input)? {
            RetryDecision::RetryNew { delay_millis } => Some(DeveloperRetryRecord::new(
                (self.turn, attempt, self.maximum, elapsed_millis, delay_millis),
                retry_after_millis,
                reason,
            )),
            RetryDecision::Stop(_) | RetryDecision::Resume { .. } => None,
        })
    }
}

fn deterministic_jitter(request_prefix: &str, turn: u16, attempt: u8) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in request_prefix.bytes().chain(turn.to_le_bytes()).chain(core::iter::once(attempt)) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash % (MAX_JITTER_MILLIONTHS + 1)
}
