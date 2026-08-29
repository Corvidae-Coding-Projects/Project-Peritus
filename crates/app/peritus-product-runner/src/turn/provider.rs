//! Product-role recovery after one developer invocation reaches a provider terminal.

use peritus_agent::{DeveloperLoopError, DeveloperLoopOutcome};

use crate::ProductRunnerError;
use crate::budget::RunAccounting;
use crate::execution::ProductRunInput;
use crate::failover::{ProviderCursor, RoleRecovery};
use crate::progress::WorkspaceCheckpoint;

use super::{DeveloperInvocation, developer_error};

pub(super) enum ProviderResolution {
    Outcome(DeveloperLoopOutcome),
    Retry(Option<&'static str>),
}

pub(super) fn resolve(
    input: &ProductRunInput,
    providers: &mut ProviderCursor<'_>,
    identity: DeveloperInvocation<'_>,
    result: Result<DeveloperLoopOutcome, DeveloperLoopError>,
    checkpoint: &mut WorkspaceCheckpoint,
    recovery: &mut RoleRecovery,
    accounting: &mut RunAccounting,
) -> Result<ProviderResolution, ProductRunnerError> {
    let error = match result {
        Ok(outcome) => {
            recovery.reset();
            return Ok(ProviderResolution::Outcome(outcome));
        }
        Err(error) => error,
    };
    let current = WorkspaceCheckpoint::capture(&input.workspace_root)?;
    if current != *checkpoint {
        *checkpoint = current;
        recovery.reset();
        return Ok(ProviderResolution::Retry(None));
    }
    if let Some(reason) = recovery.retry(&error) {
        return Ok(ProviderResolution::Retry(Some(reason)));
    }
    if let Some(switch) = providers.advance(&error) {
        crate::failover::record_switch(input, identity.role, identity.cycle, accounting, switch)?;
        recovery.reset();
        return Ok(ProviderResolution::Retry(None));
    }
    Err(developer_error(&error))
}

pub(super) fn apply(
    resolution: ProviderResolution,
    correction: &mut Option<String>,
    pending_question: &mut Option<String>,
    unproductive_terminals: &mut u8,
) -> Option<DeveloperLoopOutcome> {
    match resolution {
        ProviderResolution::Outcome(result) => Some(result),
        ProviderResolution::Retry(Some(reason)) => {
            *correction = Some(RoleRecovery::correction(reason));
            *pending_question = None;
            None
        }
        ProviderResolution::Retry(None) => {
            *unproductive_terminals = 0;
            (*correction, *pending_question) = (None, None);
            None
        }
    }
}

pub(super) fn record_accounting(
    result: &Result<DeveloperLoopOutcome, DeveloperLoopError>,
    accounting: &mut RunAccounting,
) -> Result<(), ProductRunnerError> {
    match result {
        Ok(outcome) => accounting.record(outcome),
        Err(_) => accounting.check(),
    }
}
