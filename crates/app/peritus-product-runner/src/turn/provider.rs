//! Product-role recovery after one developer invocation reaches a provider terminal.

use peritus_agent::{DeveloperLoopError, DeveloperLoopOutcome};

use crate::ProductRunnerError;
use crate::budget::RunAccounting;
use crate::execution::ProductRunInput;
use crate::failover::{ProviderCursor, RoleRecovery};

use super::{DeveloperInvocation, developer_error};

pub(super) enum ProviderResolution {
    Outcome(DeveloperLoopOutcome),
    Retry(&'static str),
    ContinueSegment,
}

pub(super) fn resolve(
    input: &ProductRunInput,
    providers: &mut ProviderCursor<'_>,
    identity: DeveloperInvocation<'_>,
    result: Result<DeveloperLoopOutcome, DeveloperLoopError>,
    recovery: &mut RoleRecovery,
    accounting: &mut RunAccounting,
) -> Result<ProviderResolution, ProductRunnerError> {
    let error = match result {
        Ok(outcome) => {
            crate::failover::record_provider_success(recovery);
            return Ok(ProviderResolution::Outcome(outcome));
        }
        Err(error) => error,
    };
    if matches!(error, DeveloperLoopError::SegmentContinuation) {
        recovery.reset();
        return Ok(ProviderResolution::ContinueSegment);
    }
    if matches!(error, DeveloperLoopError::SegmentExhausted) {
        recovery.reset();
        accounting.record_role_retry()?;
        return Ok(ProviderResolution::Retry("segment_boundary"));
    }
    if let Some(reason) = recovery.retry(&error) {
        accounting.record_role_retry()?;
        return Ok(ProviderResolution::Retry(reason));
    }
    if let Some(switch) = providers.advance(&error) {
        crate::failover::record_switch(input, identity.role, identity.cycle, accounting, switch)?;
        recovery.reset();
        return Ok(ProviderResolution::Retry("provider_transfer"));
    }
    Err(developer_error(&error))
}

pub(super) fn apply(
    resolution: ProviderResolution,
    correction: &mut Option<String>,
    pending_question: &mut Option<String>,
) -> Option<DeveloperLoopOutcome> {
    match resolution {
        ProviderResolution::Outcome(result) => Some(result),
        ProviderResolution::Retry("provider_transfer") => {
            *correction = Some(RoleRecovery::transfer_correction(correction.as_deref()));
            *pending_question = None;
            None
        }
        ProviderResolution::Retry(reason) => {
            *correction = Some(RoleRecovery::correction(reason));
            *pending_question = None;
            None
        }
        ProviderResolution::ContinueSegment => None,
    }
}
