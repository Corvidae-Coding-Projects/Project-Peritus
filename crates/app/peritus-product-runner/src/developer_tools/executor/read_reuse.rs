//! Reused read admission stays inside the normal workspace authority boundary.

use super::{
    DeveloperLoopError, DeveloperToolObservation, Value, WorkspaceDeveloperTools, checked,
};
use peritus_model_protocol::CompletedToolCall;

impl WorkspaceDeveloperTools {
    pub(super) fn dispatch_reusing_read(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        reused: Option<&DeveloperToolObservation>,
    ) -> Result<Value, DeveloperLoopError> {
        let candidate =
            reused.filter(|observation| !observation.is_error).and_then(|observation| {
                serde_json::from_slice::<Value>(observation.output.canonical_bytes()).ok()
            });
        let Some(mut candidate) = candidate.filter(|candidate| {
            call.name().as_str() == "workspace_read"
                && arguments.get("path") == candidate.get("path")
                && candidate.get("read_reuse").is_some()
        }) else {
            return self.dispatch_tool(call, arguments);
        };
        self.inspection_cancellation.check()?;
        // Reopen even on a metadata/hash-cache hit so reuse cannot bypass revoked OS access.
        let path = checked(&self.root, arguments["path"].as_str().unwrap_or(""), true)?;
        let _readable =
            std::fs::File::open(&path).map_err(|error| super::tool(error.to_string()))?;
        let digest = super::super::grounding::file_digest(&path)
            .map_err(|error| super::tool(error.to_string()))?;
        if candidate["source_sha256"].as_str() != Some(digest.as_str()) {
            return self.dispatch_tool(call, arguments);
        }
        if candidate.get("content").is_some() {
            let metadata = _readable.metadata().map_err(|error| super::tool(error.to_string()))?;
            candidate["permissions"] = Value::String(crate::file_metadata::permissions(&metadata));
            candidate["bytes"] = Value::from(metadata.len());
        }
        if candidate["read_reuse"]["state"] != "read_missing" {
            return match crate::local_context::read_reuse::fit_response(candidate, arguments)? {
                Some(value) => Ok(value),
                None => self.dispatch_tool(call, arguments),
            };
        }
        let mut narrowed = arguments.clone();
        for key in ["start_line", "end_line", "line_byte_offset", "end_line_byte_offset"] {
            if let Some(value) = candidate["read_reuse"]["arguments"].get(key) {
                narrowed[key] = value.clone();
            }
        }
        let mut value =
            super::super::inspection::read(&self.root, &narrowed, &self.inspection_cancellation)?;
        // An external edit between admission and dispatch also invalidates the skipped ranges.
        if value["source_sha256"] != candidate["source_sha256"] {
            return self.dispatch_tool(call, arguments);
        }
        value["read_reuse"] = candidate["read_reuse"].clone();
        match crate::local_context::read_reuse::fit_response(value, arguments)? {
            Some(value) => Ok(value),
            None => self.dispatch_tool(call, arguments),
        }
    }
}
