//! Run-owned accounting is updated before another provider or tool boundary can fail.

use super::FileDeveloperTrace;
use crate::budget::RunAccounting;
use peritus_agent::{
    DeveloperAccountingEvent, DeveloperLoopError, DeveloperTrace, DeveloperTraceEvent,
};
use std::path::Path;

pub struct AccountingTrace<'a> {
    pub(crate) trace: FileDeveloperTrace,
    accounting: &'a mut RunAccounting,
}

impl<'a> AccountingTrace<'a> {
    pub(crate) fn new(path: &Path, accounting: &'a mut RunAccounting) -> Self {
        Self { trace: FileDeveloperTrace::new(path.to_owned()), accounting }
    }
}

impl DeveloperTrace for AccountingTrace<'_> {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        self.trace.record(event)
    }

    fn account(&mut self, event: DeveloperAccountingEvent) -> Result<(), DeveloperLoopError> {
        self.accounting.record_event(event).map_err(|error| {
            self.accounting.retain_failure(error);
            DeveloperLoopError::RecoveryRequired(
                "host accounting rejected the boundary; retain the original run failure".to_owned(),
            )
        })
    }

    fn recover_retry(
        &mut self,
        request_prefix: &str,
        turn: u16,
        provider_profile_id: peritus_types::ProviderProfileId,
        native_session_digest: Option<peritus_types::Sha256Digest>,
        request_fingerprint: peritus_types::Sha256Digest,
    ) -> Result<Option<peritus_agent::DeveloperRetryRecovery>, DeveloperLoopError> {
        self.trace.recover_retry(
            request_prefix,
            turn,
            provider_profile_id,
            native_session_digest,
            request_fingerprint,
        )
    }

    fn begin_retry_attempt(
        &mut self,
        turn: u16,
        attempt: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        self.trace.begin_retry_attempt(turn, attempt, request)
    }

    fn finish_retry_attempt(
        &mut self,
        turn: u16,
        attempt: u64,
        request: &peritus_model_protocol::ModelRequest,
        disposition: peritus_agent::DeveloperRetryDisposition,
    ) -> Result<(), DeveloperLoopError> {
        self.trace.finish_retry_attempt(turn, attempt, request, disposition)
    }

    fn supersede_retry(
        &mut self,
        request_prefix: &str,
        turn: u16,
        scheduled_attempt: u64,
    ) -> Result<(), DeveloperLoopError> {
        self.trace.supersede_retry(request_prefix, turn, scheduled_attempt)
    }

    fn supersede_retry_for_provider_selection(
        &mut self,
        request_prefix: &str,
        turn: u16,
        current_selection: peritus_types::Sha256Digest,
    ) -> Result<bool, DeveloperLoopError> {
        self.trace.supersede_retry_for_provider_selection(
            request_prefix,
            turn,
            current_selection,
        )
    }
}
