//! Raw tool-fragment ownership, exact terminal binding, and closed-argument normalization.

use super::{
    ItemKind, ModelEvent, ProviderCoreError, ResponsesDecoder, Value, error, index, string,
};

impl ResponsesDecoder {
    pub(super) fn tool_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let id = string(value, "item_id")?;
        let index = index(value, "output_index")?;
        let bytes = string(value, "delta")?.as_bytes();
        let item = self
            .state
            .item_mut(id)
            .ok_or_else(|| error::malformed("Responses-compatible tool delta preceded its item"))?;
        if item.kind != ItemKind::ToolCall
            || item.index != index
            || item.value_done
            || item.completed
        {
            return Err(error::malformed("Responses-compatible tool delta targeted wrong item"));
        }
        item.bytes.append(bytes, self.limits)?;
        Ok(Vec::new())
    }

    pub(super) fn tool_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let id = string(value, "item_id")?;
        let index = index(value, "output_index")?;
        let complete = string(value, "arguments")?.as_bytes();
        let item = self.state.item_mut(id).ok_or_else(|| {
            error::malformed("Responses-compatible tool terminal preceded its item")
        })?;
        if item.kind != ItemKind::ToolCall
            || item.index != index
            || item.value_done
            || item.bytes.as_bytes() != complete
        {
            return Err(error::malformed("Responses-compatible completed tool input changed"));
        }
        item.value_done = true;
        let call_id = item.call_id.as_ref().ok_or_else(|| {
            error::malformed("Responses-compatible tool item omitted call identity")
        })?;
        item.bytes.complete(call_id, self.limits)
    }
}
