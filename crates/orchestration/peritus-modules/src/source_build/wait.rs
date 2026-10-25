//! Scriptable waiting that does not let a short-lived CLI exit before owner cleanup.

use super::{active, active_key};
use crate::{Error, Registry, invalid};
use serde_json::Value;
use std::{thread, time::Duration};

pub(super) fn terminal(registry: &Registry, id: &str) -> Result<Value, Error> {
    loop {
        let record = registry
            .source_build_records()?
            .into_iter()
            .find(|record| record["module_id"] == id)
            .ok_or_else(|| invalid("source build record is unavailable"))?;
        let terminal = record["phase"].as_str().is_some_and(|phase| {
            matches!(phase, "installed" | "failed" | "cancelled" | "interrupted")
        });
        let active = active()
            .lock()
            .map_err(|_| invalid("source-build ownership is poisoned"))?
            .contains_key(&active_key(registry.root(), id));
        if terminal && !active {
            return Ok(record);
        }
        thread::sleep(Duration::from_millis(200));
    }
}
