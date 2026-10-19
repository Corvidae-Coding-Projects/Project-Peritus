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
    path: std::path::PathBuf,
    invocation: String,
}

impl<'a> AccountingTrace<'a> {
    pub(crate) fn new(path: &Path, accounting: &'a mut RunAccounting, invocation: &str) -> Self {
        Self {
            trace: FileDeveloperTrace::new(path.to_owned()),
            accounting,
            path: path.to_owned(),
            invocation: invocation.to_owned(),
        }
    }
}

impl DeveloperTrace for AccountingTrace<'_> {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        self.trace.record(event)
    }

    fn account(&mut self, event: DeveloperAccountingEvent) -> Result<(), DeveloperLoopError> {
        self.accounting.record_event(event).map_err(|_| DeveloperLoopError::LimitExceeded)?;
        if matches!(
            event,
            DeveloperAccountingEvent::Usage(_) | DeveloperAccountingEvent::ModelRequest { .. }
        ) {
            let progress = self.accounting.latest_snapshot();
            let response_usage = match event {
                DeveloperAccountingEvent::Usage(usage) => serde_json::json!({
                    "input_tokens":usage.input_tokens(),"cached_input_tokens":usage.cached_input_tokens(),
                    "output_tokens":usage.output_tokens(),
                }),
                _ => serde_json::Value::Null,
            };
            crate::efficiency::event(
                &self.path,
                serde_json::json!({"event":"provider-accounting","invocation":self.invocation,
                "response_usage":response_usage,
                "model_requests":progress.model_requests(),"input_tokens":progress.input_tokens(),
                "cached_input_tokens":progress.cached_input_tokens(),"output_tokens":progress.output_tokens(),
                "semantics":"cumulative normalized snapshots; replace, do not sum snapshots"}),
            );
        }
        Ok(())
    }
}
