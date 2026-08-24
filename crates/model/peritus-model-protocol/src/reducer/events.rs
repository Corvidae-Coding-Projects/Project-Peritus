//! Started-response event application and item assembly.

use super::{ItemAssembly, ReducedItem, ReducerTransition, ResponseReducer, ToolAssembly};
use crate::{
    FragmentCompletionFacts, ItemId, ItemKind, ModelEvent, ProtocolError, ResponseId, ToolCallId,
};

impl ResponseReducer {
    pub(super) fn apply_started(
        &mut self,
        event: ModelEvent,
    ) -> Result<ReducerTransition, ProtocolError> {
        match event {
            ModelEvent::ResponseIdentity(id) => self.observe_response_id(id)?,
            ModelEvent::ItemStarted { item_id, index, kind } => {
                self.start_item(item_id, index, kind)?;
            }
            ModelEvent::TextDelta { item_id, fragment } => {
                self.append_item(
                    &item_id,
                    fragment.expose(),
                    &[ItemKind::Message, ItemKind::StructuredOutput, ItemKind::ProviderNative],
                )?;
            }
            ModelEvent::ReasoningSummaryDelta { item_id, fragment } => {
                self.append_item(&item_id, fragment.expose(), &[ItemKind::Reasoning])?;
            }
            ModelEvent::ReasoningReplayDelta { item_id, fragment } => {
                self.append_replay(&item_id, fragment.expose())?;
            }
            ModelEvent::RefusalDelta { item_id, fragment } => {
                self.append_item(&item_id, fragment.expose(), &[ItemKind::Refusal])?;
            }
            ModelEvent::ToolCallStarted { item_id, call_id, name } => {
                self.start_call(item_id, call_id, name)?;
            }
            ModelEvent::ToolArgumentDelta { call_id, fragment } => {
                self.append_arguments(&call_id, fragment.expose())?;
            }
            ModelEvent::ItemCompleted(item_id) => self.complete_item(&item_id)?,
            ModelEvent::Usage(observation) => {
                if self.usage.observe(&observation).is_err() {
                    return self.reject("usage observation regressed or followed final usage");
                }
            }
            ModelEvent::RateLimit(observation) => {
                if self.rate_limits.len() >= 64 {
                    return self.reject("rate-limit observation count exceeds its bound");
                }
                self.rate_limits.push(observation);
            }
            ModelEvent::Cache(observation) => {
                if self.cache.len() >= 64 {
                    return self.reject("cache observation count exceeds its bound");
                }
                self.cache.push(observation);
            }
            ModelEvent::Finish(reason) => {
                if self.finish.replace(reason).is_some() {
                    return self.reject("finish reason appeared more than once");
                }
            }
            ModelEvent::ProviderEvent(extension) => {
                if self.extensions.len() >= 128 {
                    return self.reject("provider event count exceeds its bound");
                }
                self.extensions.push(extension);
            }
            ModelEvent::ResponseCompleted => return self.complete_response(),
            ModelEvent::Heartbeat
            | ModelEvent::ResponseStarted { .. }
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled => {
                return self.reject("event is illegal in the started response phase");
            }
        }
        Ok(ReducerTransition::Applied)
    }

    fn observe_response_id(&mut self, id: ResponseId) -> Result<(), ProtocolError> {
        if self.response_id.as_ref().is_some_and(|current| current != &id) {
            return self.reject_unit("provider response identity changed");
        }
        self.response_id = Some(id);
        Ok(())
    }

    fn start_item(
        &mut self,
        item_id: ItemId,
        index: u32,
        kind: ItemKind,
    ) -> Result<(), ProtocolError> {
        if self.items.len() >= self.limits.max_items()
            || self.items.contains_key(&item_id)
            || !self.indexes.insert(index)
        {
            return self.reject_unit("item identity/index is duplicate or exceeds its bound");
        }
        self.items.insert(
            item_id,
            ItemAssembly {
                index,
                kind,
                content: Vec::new(),
                replay: Vec::new(),
                call: None,
                complete: false,
            },
        );
        Ok(())
    }

    fn append_item(
        &mut self,
        item_id: &ItemId,
        bytes: &[u8],
        allowed: &[ItemKind],
    ) -> Result<(), ProtocolError> {
        self.account_output(bytes.len())?;
        let Some(item) = self.items.get_mut(item_id) else {
            return self.reject_unit("delta preceded item start");
        };
        if item.complete || !allowed.contains(&item.kind) {
            return self.reject_unit("delta targeted a completed or incompatible item");
        }
        item.content.extend_from_slice(bytes);
        Ok(())
    }

    fn append_replay(&mut self, item_id: &ItemId, bytes: &[u8]) -> Result<(), ProtocolError> {
        self.account_output(bytes.len())?;
        let Some(item) = self.items.get_mut(item_id) else {
            return self.reject_unit("replay delta preceded item start");
        };
        if item.complete || item.kind != ItemKind::Reasoning {
            return self.reject_unit("replay delta targeted a completed or non-reasoning item");
        }
        if item.replay.len().saturating_add(bytes.len()) > self.limits.max_extension_bytes() {
            return self.reject_unit("reasoning replay state exceeds its byte bound");
        }
        item.replay.extend_from_slice(bytes);
        Ok(())
    }

    fn start_call(
        &mut self,
        item_id: ItemId,
        call_id: ToolCallId,
        name: crate::ToolName,
    ) -> Result<(), ProtocolError> {
        if self.calls.contains_key(&call_id) {
            return self.reject_unit("tool-call identity is duplicate");
        }
        let Some(item) = self.items.get_mut(&item_id) else {
            return self.reject_unit("tool call preceded item start");
        };
        if item.complete || item.kind != ItemKind::ToolCall || item.call.is_some() {
            return self.reject_unit("tool call targeted a completed or incompatible item");
        }
        item.call = Some(ToolAssembly { id: call_id.clone(), name, arguments: Vec::new() });
        self.calls.insert(call_id, item_id);
        Ok(())
    }

    fn append_arguments(
        &mut self,
        call_id: &ToolCallId,
        bytes: &[u8],
    ) -> Result<(), ProtocolError> {
        self.account_output(bytes.len())?;
        let Some(item_id) = self.calls.get(call_id) else {
            return self.reject_unit("tool arguments preceded call start");
        };
        let item_id = item_id.clone();
        let Some(call) = self.items.get_mut(&item_id).and_then(|item| item.call.as_mut()) else {
            return self.reject_unit("tool-call assembly is missing");
        };
        if call.arguments.len().saturating_add(bytes.len()) > self.limits.max_tool_argument_bytes()
        {
            return self.reject_unit("tool arguments exceed their byte bound");
        }
        call.arguments.extend_from_slice(bytes);
        Ok(())
    }

    fn complete_item(&mut self, item_id: &ItemId) -> Result<(), ProtocolError> {
        let Some(item) = self.items.get(item_id).cloned() else {
            return self.reject_unit("item completion preceded item start");
        };
        if item.complete {
            return self.reject_unit("item completed more than once");
        }
        let reduced = match self.reduce_item(item_id.clone(), &item) {
            Ok(reduced) => reduced,
            Err(error) => {
                self.mark_malformed();
                return Err(error);
            }
        };
        let call_bytes_bounded = item
            .call
            .as_ref()
            .is_none_or(|call| call.arguments.len() <= self.limits.max_output_bytes());
        if !crate::verified::fragment_completion_legal(FragmentCompletionFacts {
            bytes_bounded: item.content.len() <= self.limits.max_output_bytes()
                && item.replay.len() <= self.limits.max_extension_bytes()
                && call_bytes_bounded,
            // Successful ReducedItem construction proves the applicable UTF-8 and JSON checks.
            utf8_complete: true,
            json_complete: true,
            // This path is reachable only from an explicit ItemCompleted event.
            explicitly_closed: true,
        }) {
            return self.reject_unit("completed item contradicted its formal fragment projection");
        }
        if let Some(current) = self.items.get_mut(item_id) {
            current.complete = true;
        }
        let position = self
            .completed
            .binary_search_by_key(&reduced.index(), ReducedItem::index)
            .unwrap_or_else(|position| position);
        self.completed.insert(position, reduced);
        Ok(())
    }
}
