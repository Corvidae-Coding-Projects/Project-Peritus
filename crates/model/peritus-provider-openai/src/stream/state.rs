//! Private item/content assembly and provider sequence state.

use std::collections::BTreeMap;

use peritus_model_protocol::{
    EventEnvelope, ItemId, ItemKind, ModelEvent, ModelName, ProtocolLimits, ResponseId, ToolCallId,
    ToolName,
};
use peritus_provider_core::ProviderCoreError;
use peritus_types::Sha256Digest;
use serde_json::Value;

use crate::error;

pub(super) struct ResponseState {
    response_id: Option<ResponseId>,
    started: bool,
    last_provider_sequence: Option<u64>,
    seen_sequences: BTreeMap<u64, Sha256Digest>,
    items: BTreeMap<String, ItemState>,
    parts: BTreeMap<(String, u32), PartState>,
}

pub(super) struct ItemState {
    pub normalized_id: ItemId,
    pub output_index: u32,
    pub kind: ItemKind,
    pub call_id: Option<ToolCallId>,
    pub call_name: Option<ToolName>,
    pub arguments: peritus_provider_core::healing::ToolArgumentBuffer,
    pub argument_progress_revision: u64,
    pub arguments_done: bool,
    pub completed: bool,
}

pub(super) struct PartState {
    pub normalized_id: ItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub kind: ItemKind,
    pub bytes: Vec<u8>,
    pub progress_revision: u64,
    pub value_done: bool,
    pub completed: bool,
}

impl ResponseState {
    pub const fn new() -> Self {
        Self {
            response_id: None,
            started: false,
            last_provider_sequence: None,
            seen_sequences: BTreeMap::new(),
            items: BTreeMap::new(),
            parts: BTreeMap::new(),
        }
    }

    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.response_id.as_ref()
    }

    pub const fn started(&self) -> bool {
        self.started
    }

    pub fn start(&mut self, response_id: ResponseId) -> bool {
        if self.started {
            return false;
        }
        self.started = true;
        self.response_id = Some(response_id);
        true
    }

    pub fn response_matches(&self, response_id: &str) -> bool {
        self.response_id.as_ref().is_some_and(|known| known.expose_for_wire() == response_id)
    }

    pub fn observe_sequence(&mut self, sequence: u64, digest: Sha256Digest) -> SequenceDisposition {
        if let Some(seen) = self.seen_sequences.get(&sequence) {
            return if *seen == digest {
                SequenceDisposition::Duplicate
            } else {
                SequenceDisposition::Conflict
            };
        }
        if sequence == 0 || self.last_provider_sequence.is_some_and(|last| sequence <= last) {
            return SequenceDisposition::Conflict;
        }
        self.last_provider_sequence = Some(sequence);
        self.seen_sequences.insert(sequence, digest);
        SequenceDisposition::New
    }

    pub fn restore(
        prefix: &[EventEnvelope],
        response_id: &ResponseId,
        provider_sequence: u64,
        expected_model: &ModelName,
        limits: ProtocolLimits,
    ) -> Result<Option<Self>, ProviderCoreError> {
        let mut restored = Self::new();
        for (index, envelope) in prefix.iter().enumerate() {
            if let Some(sequence) = envelope.provider_sequence() {
                match restored.observe_sequence(sequence, envelope.provider_digest()) {
                    SequenceDisposition::Conflict => {
                        return Err(error::malformed(
                            "persisted OpenAI provider sequence is conflicting or reordered",
                        ));
                    }
                    SequenceDisposition::Duplicate => continue,
                    SequenceDisposition::New => {}
                }
                if matches!(envelope.event(), ModelEvent::Heartbeat) {
                    let completes_replayed_frame = index.checked_sub(1).is_some_and(|previous| {
                        let previous = &prefix[previous];
                        previous.provider_sequence().is_none()
                            && previous.provider_digest() == envelope.provider_digest()
                            && !matches!(previous.event(), ModelEvent::Heartbeat)
                    });
                    if !completes_replayed_frame {
                        return Ok(None);
                    }
                }
            }
            restored.restore_event(envelope.event(), response_id, expected_model, limits)?;
        }
        if !restored.started
            || !restored.response_matches(response_id.expose_for_wire())
            || restored.last_provider_sequence != Some(provider_sequence)
        {
            return Err(error::malformed(
                "persisted OpenAI decoder state disagrees with its continuation boundary",
            ));
        }
        Ok(Some(restored))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "restoration mirrors the closed normalized event grammar in one auditable table"
    )]
    fn restore_event(
        &mut self,
        event: &ModelEvent,
        response_id: &ResponseId,
        expected_model: &ModelName,
        limits: ProtocolLimits,
    ) -> Result<(), ProviderCoreError> {
        match event {
            ModelEvent::ResponseStarted { response_id: Some(identity), model: Some(model) } => {
                if identity != response_id || model != expected_model || !self.start(identity.clone())
                {
                    return Err(error::malformed(
                        "persisted OpenAI response start changed response or model authority",
                    ));
                }
            }
            ModelEvent::ItemStarted { item_id, index, kind } => {
                self.restore_item_start(item_id, *index, *kind)?;
            }
            ModelEvent::ToolCallStarted { item_id, call_id, name } => {
                let item = self
                    .items
                    .values_mut()
                    .find(|item| &item.normalized_id == item_id)
                    .ok_or_else(|| error::malformed("persisted OpenAI tool start lost its item"))?;
                if item.kind != ItemKind::ToolCall
                    || item.call_id.is_some()
                    || item.call_name.is_some()
                {
                    return Err(error::malformed(
                        "persisted OpenAI tool start contradicted its item",
                    ));
                }
                item.call_id = Some(call_id.clone());
                item.call_name = Some(name.clone());
            }
            ModelEvent::TextDelta { item_id, fragment } => {
                let part = self.part_by_normalized_mut(item_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI text fragment lost its content part")
                })?;
                if !matches!(part.kind, ItemKind::Message | ItemKind::StructuredOutput)
                    || part.completed
                {
                    return Err(error::malformed(
                        "persisted OpenAI text fragment targeted incompatible state",
                    ));
                }
                if part.kind == ItemKind::StructuredOutput {
                    if part.progress_revision == 0 {
                        append_restored(&mut part.bytes, fragment.expose(), limits.max_output_bytes())?;
                    }
                    part.value_done = true;
                } else {
                    append_restored(&mut part.bytes, fragment.expose(), limits.max_output_bytes())?;
                }
            }
            ModelEvent::StructuredOutputProgress { item_id, revision, fragment } => {
                let part = self.part_by_normalized_mut(item_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI structured progress lost its content part")
                })?;
                let expected_revision = part.progress_revision.checked_add(1).ok_or_else(|| {
                    error::limit("persisted OpenAI structured progress revision overflowed")
                })?;
                if part.kind != ItemKind::StructuredOutput
                    || part.completed
                    || part.value_done
                    || *revision != expected_revision
                {
                    return Err(error::malformed(
                        "persisted OpenAI structured progress is out of order",
                    ));
                }
                append_restored(&mut part.bytes, fragment.expose(), limits.max_output_bytes())?;
                part.progress_revision = *revision;
            }
            ModelEvent::RefusalDelta { item_id, fragment } => {
                let part = self.part_by_normalized_mut(item_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI refusal fragment lost its content part")
                })?;
                if part.kind != ItemKind::Refusal || part.completed || part.value_done {
                    return Err(error::malformed(
                        "persisted OpenAI refusal fragment targeted incompatible state",
                    ));
                }
                append_restored(&mut part.bytes, fragment.expose(), limits.max_output_bytes())?;
            }
            ModelEvent::ToolArgumentProgress { call_id, revision, fragment } => {
                let item = self.item_by_call_mut(call_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI tool progress lost its call")
                })?;
                let expected_revision =
                    item.argument_progress_revision.checked_add(1).ok_or_else(|| {
                        error::limit("persisted OpenAI tool progress revision overflowed")
                    })?;
                if item.completed
                    || item.arguments_done
                    || *revision != expected_revision
                {
                    return Err(error::malformed(
                        "persisted OpenAI tool progress is out of order",
                    ));
                }
                item.arguments.append(fragment.expose(), limits)?;
                item.argument_progress_revision = *revision;
            }
            ModelEvent::ToolArgumentDelta { call_id, fragment } => {
                let item = self.item_by_call_mut(call_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI final tool arguments lost their call")
                })?;
                if item.completed {
                    return Err(error::malformed(
                        "persisted OpenAI final tool arguments followed item completion",
                    ));
                }
                if item.argument_progress_revision == 0 {
                    item.arguments.append(fragment.expose(), limits)?;
                }
                item.arguments_done = true;
            }
            ModelEvent::ItemCompleted(item_id) => self.restore_item_completed(item_id)?,
            ModelEvent::ProviderEvent(extension)
                if extension.name().as_str() == "openai.ancillary" =>
            {
                let value: Value = serde_json::from_slice(extension.value().canonical_bytes())
                    .map_err(|_| {
                        error::malformed("persisted OpenAI ancillary state is not valid JSON")
                    })?;
                self.restore_ancillary(&value)?;
            }
            ModelEvent::ResponseStarted { .. }
            | ModelEvent::ResponseIdentity(_)
            | ModelEvent::ResponseRejected(_)
            | ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled => {
                return Err(error::malformed(
                    "persisted OpenAI continuation contains incompatible lifecycle state",
                ));
            }
            ModelEvent::ReasoningSummaryDelta { .. }
            | ModelEvent::ReasoningReplayDelta { .. }
            | ModelEvent::Usage(_)
            | ModelEvent::RateLimit(_)
            | ModelEvent::Cache(_)
            | ModelEvent::Finish(_)
            | ModelEvent::ProviderEvent(_)
            | ModelEvent::OptionalObservation(_)
            | ModelEvent::Heartbeat => {}
        }
        Ok(())
    }

    fn restore_item_start(
        &mut self,
        item_id: &ItemId,
        index: u32,
        kind: ItemKind,
    ) -> Result<(), ProviderCoreError> {
        let output_index = index / 65_536;
        let content_index = index % 65_536;
        if matches!(kind, ItemKind::Message | ItemKind::StructuredOutput | ItemKind::Refusal) {
            let wire_id = restored_wire_id(item_id, content_index)?;
            if let Some(parent) = self.items.get(&wire_id) {
                if parent.output_index != output_index || parent.kind != ItemKind::Message {
                    return Err(error::malformed(
                        "persisted OpenAI content part contradicted its parent item",
                    ));
                }
            } else {
                let normalized_id = ItemId::new(wire_id.clone()).map_err(|_| {
                    error::malformed("persisted OpenAI parent item identity is invalid")
                })?;
                self.items.insert(
                    wire_id.clone(),
                    ItemState {
                        normalized_id,
                        output_index,
                        kind: ItemKind::Message,
                        call_id: None,
                        call_name: None,
                        arguments: peritus_provider_core::healing::ToolArgumentBuffer::new(),
                        argument_progress_revision: 0,
                        arguments_done: false,
                        completed: false,
                    },
                );
            }
            if !self.insert_part(
                wire_id,
                content_index,
                PartState {
                    normalized_id: item_id.clone(),
                    output_index,
                    content_index,
                    kind,
                    bytes: Vec::new(),
                    progress_revision: 0,
                    value_done: false,
                    completed: false,
                },
            ) {
                return Err(error::malformed(
                    "persisted OpenAI content part was started more than once",
                ));
            }
            return Ok(());
        }
        let wire_id = item_id.expose_for_wire().to_owned();
        if !self.insert_item(
            wire_id,
            ItemState {
                normalized_id: item_id.clone(),
                output_index,
                kind,
                call_id: None,
                call_name: None,
                arguments: peritus_provider_core::healing::ToolArgumentBuffer::new(),
                argument_progress_revision: 0,
                arguments_done: false,
                completed: false,
            },
        ) {
            return Err(error::malformed(
                "persisted OpenAI output item was started more than once",
            ));
        }
        Ok(())
    }

    fn restore_item_completed(&mut self, item_id: &ItemId) -> Result<(), ProviderCoreError> {
        if let Some(part) = self.part_by_normalized_mut(item_id) {
            if part.completed || !part.value_done {
                return Err(error::malformed(
                    "persisted OpenAI content completion is out of order",
                ));
            }
            part.completed = true;
            return Ok(());
        }
        let item = self
            .items
            .values_mut()
            .find(|item| &item.normalized_id == item_id)
            .ok_or_else(|| error::malformed("persisted OpenAI item completion lost its item"))?;
        if item.completed
            || (item.kind == ItemKind::ToolCall
                && (!item.arguments_done || item.call_id.is_none() || item.call_name.is_none()))
        {
            return Err(error::malformed(
                "persisted OpenAI item completion is out of order",
            ));
        }
        item.completed = true;
        Ok(())
    }

    fn restore_ancillary(&mut self, value: &Value) -> Result<(), ProviderCoreError> {
        let Some(kind) = value.get("type").and_then(Value::as_str) else { return Ok(()) };
        match kind {
            "response.output_item.added" => {
                let output_index = json_u32(value, "output_index")?;
                let wire = json_object(value, "item")?;
                if json_str(wire, "type")? != "message" {
                    return Err(error::malformed(
                        "persisted silent OpenAI item start was not a message",
                    ));
                }
                let wire_id = json_str(wire, "id")?;
                let normalized_id = ItemId::new(wire_id.to_owned()).map_err(|_| {
                    error::malformed("persisted OpenAI message item identity is invalid")
                })?;
                if !self.insert_item(
                    wire_id.to_owned(),
                    ItemState {
                        normalized_id,
                        output_index,
                        kind: ItemKind::Message,
                        call_id: None,
                        call_name: None,
                        arguments: peritus_provider_core::healing::ToolArgumentBuffer::new(),
                        argument_progress_revision: 0,
                        arguments_done: false,
                        completed: false,
                    },
                ) {
                    return Err(error::malformed(
                        "persisted OpenAI message item was added more than once",
                    ));
                }
            }
            "response.output_text.done" | "response.refusal.done" => {
                let wire_id = json_str(value, "item_id")?;
                let output_index = json_u32(value, "output_index")?;
                let content_index = json_u32(value, "content_index")?;
                let refusal = kind == "response.refusal.done";
                let complete = json_str(value, if refusal { "refusal" } else { "text" })?;
                let part = self
                    .part_mut(wire_id, content_index)
                    .ok_or_else(|| error::malformed("persisted OpenAI content done lost its part"))?;
                if part.output_index != output_index
                    || part.value_done
                    || refusal != (part.kind == ItemKind::Refusal)
                    || part.bytes != complete.as_bytes()
                {
                    return Err(error::malformed(
                        "persisted OpenAI content done contradicted its fragments",
                    ));
                }
                part.value_done = true;
            }
            "response.output_item.done" => {
                let output_index = json_u32(value, "output_index")?;
                let wire = json_object(value, "item")?;
                let wire_id = json_str(wire, "id")?;
                if !self.parts_for_item_complete(wire_id) {
                    return Err(error::malformed(
                        "persisted OpenAI message ended before its content parts",
                    ));
                }
                let item = self.items.get_mut(wire_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI message completion lost its item")
                })?;
                if item.kind != ItemKind::Message
                    || item.output_index != output_index
                    || item.completed
                {
                    return Err(error::malformed(
                        "persisted OpenAI message completion contradicted its item",
                    ));
                }
                item.completed = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn part_by_normalized_mut(&mut self, item_id: &ItemId) -> Option<&mut PartState> {
        self.parts.values_mut().find(|part| &part.normalized_id == item_id)
    }

    fn item_by_call_mut(&mut self, call_id: &ToolCallId) -> Option<&mut ItemState> {
        self.items.values_mut().find(|item| item.call_id.as_ref() == Some(call_id))
    }

    pub fn insert_item(&mut self, wire_id: String, item: ItemState) -> bool {
        self.items.insert(wire_id, item).is_none()
    }

    pub fn item(&self, wire_id: &str) -> Option<&ItemState> {
        self.items.get(wire_id)
    }

    pub fn item_mut(&mut self, wire_id: &str) -> Option<&mut ItemState> {
        self.items.get_mut(wire_id)
    }

    pub fn insert_part(&mut self, wire_id: String, content_index: u32, part: PartState) -> bool {
        self.parts.insert((wire_id, content_index), part).is_none()
    }

    pub fn part_mut(&mut self, wire_id: &str, content_index: u32) -> Option<&mut PartState> {
        self.parts.get_mut(&(wire_id.to_owned(), content_index))
    }

    pub fn parts_for_item_complete(&self, wire_id: &str) -> bool {
        let parts = self.parts.iter().filter(|((item, _), _)| item == wire_id);
        let mut count = 0_usize;
        for (_, part) in parts {
            count += 1;
            if !part.completed {
                return false;
            }
        }
        count > 0
    }

    pub fn all_items_complete(&self) -> bool {
        self.items.values().all(|item| item.completed)
            && self.parts.values().all(|part| part.completed)
    }

    pub fn has_kind(&self, kind: ItemKind) -> bool {
        self.items.values().any(|item| item.kind == kind)
            || self.parts.values().any(|part| part.kind == kind)
    }
}

fn restored_wire_id(item_id: &ItemId, content_index: u32) -> Result<String, ProviderCoreError> {
    if content_index == 0 {
        return Ok(item_id.expose_for_wire().to_owned());
    }
    let suffix = format!("-part-{content_index}");
    item_id
        .expose_for_wire()
        .strip_suffix(&suffix)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| error::malformed("persisted OpenAI content identity is inconsistent"))
}

fn append_restored(
    target: &mut Vec<u8>,
    bytes: &[u8],
    maximum: usize,
) -> Result<(), ProviderCoreError> {
    if target.len().checked_add(bytes.len()).is_none_or(|total| total > maximum) {
        return Err(error::limit("persisted OpenAI fragment exceeds its aggregate bound"));
    }
    target
        .try_reserve(bytes.len())
        .map_err(|_| error::limit("persisted OpenAI fragment capacity is unavailable"))?;
    target.extend_from_slice(bytes);
    Ok(())
}

fn json_object<'a>(value: &'a Value, name: &str) -> Result<&'a Value, ProviderCoreError> {
    value
        .get(name)
        .filter(|field| field.is_object())
        .ok_or_else(|| error::malformed("persisted OpenAI ancillary object is missing"))
}

fn json_str<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| error::malformed("persisted OpenAI ancillary string is missing"))
}

fn json_u32(value: &Value, name: &str) -> Result<u32, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|field| u32::try_from(field).ok())
        .ok_or_else(|| error::malformed("persisted OpenAI ancillary index is invalid"))
}

pub(super) enum SequenceDisposition {
    New,
    Duplicate,
    Conflict,
}
