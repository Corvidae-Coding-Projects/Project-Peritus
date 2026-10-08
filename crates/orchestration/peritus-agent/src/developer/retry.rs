//! Checked retry planning and cancellation-aware backoff for developer model turns.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    IdempotencyGuarantee, ModelFailure, ModelRequest, OutcomeCertainty, RetryAfterParseStatus,
    RetryAfterUnit, RetryCause, RetryDecision, Retryability, TerminalOutcome, UnboundedRetryInput,
    plan_unbounded_retry,
};
use peritus_provider_core::{CancellationToken, ProviderCoreErrorKind, cancel_first};
use peritus_types::Sha256Digest;

use crate::ModelDriveError;

use super::{
    DeveloperLoopError, DeveloperRetryReason, DeveloperRetryRecord, DeveloperRetryRecovery,
    DeveloperTrace, DeveloperTraceEvent,
};

const BASE_DELAY_MILLIS: u64 = 250;
const MAX_DELAY_MILLIS: u64 = 30_000;
const MAX_JITTER_MILLIONTHS: u32 = 250_000;

/// One logical turn's retained request identity and cancellation-aware retry clock.
pub(super) struct DeveloperRetryPlanner<'a> {
    turn: u16,
    started: Instant,
    elapsed_before_process: u64,
    cancellation: &'a CancellationToken,
    provider_selection: Option<Sha256Digest>,
}

impl<'a> DeveloperRetryPlanner<'a> {
    pub(super) fn new(
        turn: u16,
        cancellation: &'a CancellationToken,
        provider_selection: Option<Sha256Digest>,
        recovered: Option<DeveloperRetryRecovery>,
    ) -> Result<Self, DeveloperLoopError> {
        let elapsed_before_process = match recovered {
            Some(recovered) => {
                let now = unix_millis()?;
                let elapsed_since_schedule = if now > recovered.scheduled_unix_millis() {
                    now.checked_sub(recovered.scheduled_unix_millis())
                        .ok_or(DeveloperLoopError::LimitExceeded)?
                } else {
                    0
                };
                recovered
                    .elapsed_millis()
                    .checked_add(elapsed_since_schedule)
                    .ok_or(DeveloperLoopError::LimitExceeded)?
            }
            None => 0,
        };
        Ok(Self {
            turn,
            started: Instant::now(),
            elapsed_before_process,
            cancellation,
            provider_selection,
        })
    }

    pub(super) fn terminal(
        &self,
        request: &ModelRequest,
        attempt: u64,
        terminal: Option<&TerminalOutcome>,
        _usable: bool,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        if self.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        match terminal {
            Some(TerminalOutcome::Failed(failure)) => {
                self.rejection(request, attempt, failure)
            }
            Some(_) | None => Ok(None),
        }
    }

    pub(super) fn rejection(
        &self,
        request: &ModelRequest,
        attempt: u64,
        failure: &ModelFailure,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        if self.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        if failure.retryability() != Retryability::SafeNewRequest
            || failure.certainty() != OutcomeCertainty::DefinitelyNotAccepted
        {
            return Ok(None);
        }
        let FailureRetrySchedule::Available(retry_after_millis) =
            failure_retry_schedule(failure)?
        else {
            return Ok(None);
        };
        self.plan(
            request,
            attempt,
            RetryCause::SafeNewRequest,
            retry_after_millis,
            DeveloperRetryReason::RetryableProviderResponse,
        )
    }

    pub(super) fn record(
        &self,
        record: &DeveloperRetryRecord,
        trace: &mut dyn DeveloperTrace,
    ) -> Result<(), DeveloperLoopError> {
        trace.record(DeveloperTraceEvent::RetryScheduled(record))
    }

    pub(super) async fn wait(
        &self,
        record: &DeveloperRetryRecord,
    ) -> Result<(), DeveloperLoopError> {
        wait_until_eligible(self.cancellation, record.next_eligible_unix_millis()).await
    }

    pub(super) fn error(
        &self,
        request: &ModelRequest,
        attempt: u64,
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
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        self.plan(request, attempt, cause, None, reason)
    }

    pub(super) async fn record_and_wait(
        &self,
        record: &DeveloperRetryRecord,
        trace: &mut dyn DeveloperTrace,
    ) -> Result<(), DeveloperLoopError> {
        self.record(record, trace)?;
        self.wait(record).await
    }

    fn plan(
        &self,
        request: &ModelRequest,
        attempt: u64,
        cause: RetryCause,
        retry_after_millis: Option<u64>,
        reason: DeveloperRetryReason,
    ) -> Result<Option<DeveloperRetryRecord>, DeveloperLoopError> {
        let elapsed_in_process = u64::try_from(self.started.elapsed().as_millis())
            .map_err(|_| DeveloperLoopError::LimitExceeded)?;
        let elapsed_millis = self
            .elapsed_before_process
            .checked_add(elapsed_in_process)
            .ok_or(DeveloperLoopError::LimitExceeded)?;
        let request_id_digest = peritus_codec::sha256(
            request.request_id().expose_for_wire().as_bytes(),
        );
        let input = UnboundedRetryInput {
            attempt: attempt.checked_sub(1).ok_or(DeveloperLoopError::LimitExceeded)?,
            elapsed_millis,
            max_elapsed_millis: None,
            base_delay_millis: BASE_DELAY_MILLIS,
            max_delay_millis: MAX_DELAY_MILLIS,
            jitter_millionths: deterministic_jitter(
                request_id_digest,
                self.turn,
                attempt,
            ),
            retry_after_millis,
            cause,
            guarantee: IdempotencyGuarantee::None,
            cancelled: self.cancellation.is_cancelled(),
        };
        Ok(match plan_unbounded_retry(input)? {
            RetryDecision::RetryNew { delay_millis } => {
                let now_millis = unix_millis()?;
                let next_eligible_unix_millis = now_millis
                    .checked_add(delay_millis)
                    .ok_or_else(|| {
                        DeveloperLoopError::Trace(
                            "retry eligibility exceeds the durable wall-clock representation"
                                .to_owned(),
                        )
                    })?;
                Some(DeveloperRetryRecord::new(
                    (
                        self.turn,
                        attempt,
                        request_id_digest,
                        request.fingerprint()?.digest(),
                        request.profile_id(),
                        native_session_digest(request),
                        self.provider_selection,
                    ),
                    (
                        elapsed_millis,
                        delay_millis,
                        next_eligible_unix_millis,
                    ),
                    retry_after_millis,
                    reason,
                ))
            }
            RetryDecision::Stop(_) | RetryDecision::Resume { .. } => None,
        })
    }
}

enum FailureRetrySchedule {
    Available(Option<u64>),
    Unavailable,
}

fn failure_retry_schedule(
    failure: &ModelFailure,
) -> Result<FailureRetrySchedule, DeveloperLoopError> {
    let Some(observation) = failure.retry_after_observation() else {
        return Ok(FailureRetrySchedule::Available(
            failure.retry_after_millis(),
        ));
    };
    match (observation.unit(), observation.parse_status()) {
        (RetryAfterUnit::DeltaSeconds, RetryAfterParseStatus::Parsed) => Ok(
            FailureRetrySchedule::Available(failure.retry_after_millis()),
        ),
        (
            RetryAfterUnit::HttpDate,
            RetryAfterParseStatus::Parsed | RetryAfterParseStatus::AlreadyEligible,
        ) => {
            let Some(eligible_unix_millis) = observation.eligible_unix_millis() else {
                return Ok(FailureRetrySchedule::Unavailable);
            };
            Ok(FailureRetrySchedule::Available(Some(
                eligible_unix_millis.saturating_sub(unix_millis()?),
            )))
        }
        _ => Ok(FailureRetrySchedule::Unavailable),
    }
}

pub(super) async fn wait_until_eligible(
    cancellation: &CancellationToken,
    next_eligible_unix_millis: u64,
) -> Result<(), DeveloperLoopError> {
    let now_millis = unix_millis()?;
    if next_eligible_unix_millis <= now_millis {
        return Ok(());
    }
    let delay = next_eligible_unix_millis
        .checked_sub(now_millis)
        .ok_or(DeveloperLoopError::LimitExceeded)?;
    match cancel_first(cancellation, tokio::time::sleep(Duration::from_millis(delay))).await {
        None => Err(DeveloperLoopError::Cancelled),
        Some(()) => Ok(()),
    }
}

fn unix_millis() -> Result<u64, DeveloperLoopError> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        DeveloperLoopError::Trace("system clock predates the retry journal epoch".to_owned())
    })?;
    u64::try_from(now.as_millis()).map_err(|_| {
        DeveloperLoopError::Trace(
            "system clock exceeds the durable retry representation".to_owned(),
        )
    })
}

pub(super) fn native_session_digest(request: &ModelRequest) -> Option<Sha256Digest> {
    request.local_session_directory().map(|directory| {
        peritus_codec::sha256(directory.as_os_str().as_encoded_bytes())
    })
}

fn deterministic_jitter(
    request_id_digest: Sha256Digest,
    turn: u16,
    attempt: u64,
) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in request_id_digest
        .as_bytes()
        .iter()
        .copied()
        .chain(turn.to_le_bytes())
        .chain(attempt.to_le_bytes())
    {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash % (MAX_JITTER_MILLIONTHS + 1)
}
