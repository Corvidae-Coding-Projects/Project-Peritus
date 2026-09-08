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
        self.accounting.record_event(event).map_err(|_| DeveloperLoopError::LimitExceeded)
    }
}
