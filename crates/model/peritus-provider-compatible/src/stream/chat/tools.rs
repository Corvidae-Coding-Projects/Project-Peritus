//! Fragmented Chat tool identities, arguments, and completion state.
use super::{ChatDecoder, integer, string};
use crate::error;
use peritus_model_protocol::{ItemId, ItemKind, ModelEvent, ToolCallId, ToolName};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

pub(super) struct ToolState {
    pub(super) item_id: ItemId,
    pub(super) call_id: ToolCallId,
    pub(super) name: ToolName,
    pub(super) bytes: peritus_provider_core::healing::ToolArgumentBuffer,
    pub(super) completed: bool,
}

impl ChatDecoder {
    pub(super) fn tool(
        &mut self,
        value: &Value,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        let tool_index = u32::try_from(integer(value, "index")?)
            .map_err(|_| error::malformed("Chat-compatible tool index exceeded u32"))?;
        let function = value
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| error::malformed("Chat-compatible tool delta omitted function"))?;
        if !self.tools.contains_key(&tool_index) {
            let id = ToolCallId::new(string(value, "id")?.to_owned())
                .map_err(|_| error::malformed("Chat-compatible tool-call identity was invalid"))?;
            if value.get("type").and_then(Value::as_str) != Some("function") {
                return Err(error::malformed("Chat-compatible tool type was unmapped"));
            }
            let name = ToolName::new(string(&Value::Object(function.clone()), "name")?.to_owned())
                .map_err(|_| error::malformed("Chat-compatible tool name was invalid"))?;
            let response = self.response_id.as_ref().ok_or_else(|| {
                error::malformed("Chat-compatible response identity was unavailable")
            })?;
            let item_id = ItemId::new(format!("{}-tool-{tool_index}", response.expose_for_wire()))
                .map_err(|_| error::malformed("Chat-compatible tool item identity was invalid"))?;
            events.push(ModelEvent::ItemStarted {
                item_id: item_id.clone(),
                index: tool_index.checked_add(65_536).ok_or_else(|| {
                    error::limit("Chat-compatible normalized tool index overflowed")
                })?,
                kind: ItemKind::ToolCall,
            });
            events.push(ModelEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: id.clone(),
                name: name.clone(),
            });
            self.tools.insert(
                tool_index,
                ToolState {
                    item_id,
                    call_id: id,
                    name,
                    bytes: peritus_provider_core::healing::ToolArgumentBuffer::default(),
                    completed: false,
                },
            );
        }
        let state = self
            .tools
            .get_mut(&tool_index)
            .ok_or_else(|| error::malformed("Chat-compatible tool state disappeared"))?;
        if value
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != state.call_id.expose_for_wire())
            || function
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name != state.name.as_str())
            || state.completed
        {
            return Err(error::malformed("Chat-compatible tool delta identity changed"));
        }
        if let Some(arguments) = function.get("arguments") {
            let arguments = arguments.as_str().ok_or_else(|| {
                error::malformed("Chat-compatible tool arguments were not a string")
            })?;
            if arguments.is_empty() {
                return Ok(());
            }
            state.bytes.append(arguments.as_bytes(), self.limits)?;
        }
        Ok(())
    }
}
