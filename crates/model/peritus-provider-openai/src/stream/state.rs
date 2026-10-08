//! Private item/content assembly and provider sequence state.

use std::collections::{BTreeMap, BTreeSet};

use peritus_model_protocol::{
    EventEnvelope, ItemId, ItemKind, ModelEvent, ModelName, ProtocolLimits, ResponseId, ToolCallId,
    ToolName,
};
use peritus_provider_core::{
    ProviderCoreError,
    healing::{StructuredOutputBuffer, ToolArgumentBuffer},
};
use peritus_types::Sha256Digest;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::identity;
use crate::error;

pub(super) struct ResponseState {
    response_id: Option<ResponseId>,
    started: bool,
    last_provider_sequence: Option<u64>,
    last_provider_digest: Option<Sha256Digest>,
    items: BTreeMap<String, ItemState>,
    parts: BTreeMap<(String, u32), PartState>,
    output_indexes: BTreeSet<u32>,
    normalized_ids: BTreeSet<ItemId>,
    normalized_items: BTreeMap<ItemId, String>,
    normalized_parts: BTreeMap<ItemId, (String, u32)>,
    call_items: BTreeMap<ToolCallId, String>,
    normalized_coordinates: BTreeMap<NormalizedCoordinate, u32>,
    normalized_indexes: BTreeMap<u32, NormalizedCoordinate>,
    next_derived_index: Option<u32>,
    pending_item: Option<RestoredItemStart>,
    pending_part: Option<RestoredPartStart>,
    restored_completion: Option<RestoredCompletion>,
    restored_healing: bool,
}

pub(super) struct ItemState {
    pub normalized_id: ItemId,
    pub output_index: u32,
    pub kind: ItemKind,
    pub call_id: Option<ToolCallId>,
    pub call_name: Option<ToolName>,
    pub arguments: ToolArgumentBuffer,
    pub final_arguments: Option<ToolArgumentBuffer>,
    pub argument_progress_revision: u64,
    pub arguments_done: bool,
    pub completed: bool,
    pub parts_started: usize,
    pub parts_completed: usize,
}

pub(super) struct PartState {
    pub normalized_id: ItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub kind: ItemKind,
    pub observed: ObservedValue,
    pub validated: ObservedValue,
    pub structured: Option<StructuredOutputBuffer>,
    pub progress_revision: u64,
    pub value_done: bool,
    pub completed: bool,
}

#[derive(Clone)]
pub(super) struct ObservedValue {
    hasher: Sha256,
    bytes: usize,
}

impl ObservedValue {
    pub fn new() -> Self {
        Self { hasher: Sha256::new(), bytes: 0 }
    }

    pub fn observe(
        &mut self,
        bytes: &[u8],
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= maximum)
            .ok_or_else(|| error::limit("OpenAI fragmented output exceeds its aggregate bound"))?;
        self.hasher.update(bytes);
        Ok(())
    }

    pub fn matches_bytes(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes.len() && self.digest() == peritus_codec::sha256(bytes)
    }

    pub fn byte_len(&self) -> usize {
        self.bytes
    }

    pub fn sha256(&self) -> Sha256Digest {
        self.digest()
    }

    fn matches(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.digest() == other.digest()
    }

    fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    fn digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.hasher.clone().finalize().into())
    }
}

impl Default for ObservedValue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum NormalizedCoordinate {
    Output(u32),
    Content { output: u32, content: u32 },
}

impl NormalizedCoordinate {
    const fn legacy_index(self) -> Option<u32> {
        match self {
            Self::Output(output) if output <= u16::MAX as u32 => Some(output << 16),
            Self::Content { output, content }
                if output <= u16::MAX as u32 && content <= u16::MAX as u32 =>
            {
                Some((output << 16) | content)
            }
            Self::Output(_) | Self::Content { .. } => None,
        }
    }
}

struct RestoredItemStart {
    wire_id: String,
    output_index: u32,
    kind: ItemKind,
}

struct RestoredPartStart {
    wire_id: String,
    output_index: u32,
    content_index: u32,
    kind: ItemKind,
}

#[derive(Clone, Eq, PartialEq)]
enum RestoredCompletion {
    Structured { wire_id: String, content_index: u32 },
    Tool { wire_id: String },
}

impl ResponseState {
    pub const fn new() -> Self {
        Self {
            response_id: None,
            started: false,
            last_provider_sequence: None,
            last_provider_digest: None,
            items: BTreeMap::new(),
            parts: BTreeMap::new(),
            output_indexes: BTreeSet::new(),
            normalized_ids: BTreeSet::new(),
            normalized_items: BTreeMap::new(),
            normalized_parts: BTreeMap::new(),
            call_items: BTreeMap::new(),
            normalized_coordinates: BTreeMap::new(),
            normalized_indexes: BTreeMap::new(),
            next_derived_index: Some(u32::MAX),
            pending_item: None,
            pending_part: None,
            restored_completion: None,
            restored_healing: false,
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

    pub fn sequence(&self, sequence: u64, digest: Sha256Digest) -> SequenceDisposition {
        // OpenAI defines this value as an ordered resume cursor. It does not define numeric
        // contiguity, so a larger value advances authority while a smaller value is reordered.
        match self.last_provider_sequence {
            Some(last) if sequence == last => {
                if self.last_provider_digest == Some(digest) {
                    SequenceDisposition::Duplicate
                } else {
                    SequenceDisposition::Conflict
                }
            }
            Some(last) if sequence < last => SequenceDisposition::Conflict,
            Some(_) | None => SequenceDisposition::New,
        }
    }

    pub fn commit_sequence(&mut self, sequence: u64, digest: Sha256Digest) {
        self.last_provider_sequence = Some(sequence);
        self.last_provider_digest = Some(digest);
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
            let disposition = envelope
                .provider_sequence()
                .map(|sequence| restored.sequence(sequence, envelope.provider_digest()));
            match disposition {
                Some(SequenceDisposition::Conflict) => {
                    return Err(error::malformed(
                        "persisted OpenAI provider sequence is conflicting or reordered",
                    ));
                }
                Some(SequenceDisposition::Duplicate) => continue,
                Some(SequenceDisposition::New) | None => {}
            }
            if envelope.provider_sequence().is_some()
                && matches!(envelope.event(), ModelEvent::Heartbeat)
            {
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
            restored.restore_event(envelope.event(), response_id, expected_model, limits)?;
            if let Some(sequence) = envelope.provider_sequence() {
                restored.finish_restored_frame()?;
                restored.commit_sequence(sequence, envelope.provider_digest());
            }
        }
        if !restored.started
            || !restored.response_matches(response_id.expose_for_wire())
            || restored.last_provider_sequence != Some(provider_sequence)
            || restored.pending_item.is_some()
            || restored.pending_part.is_some()
            || restored.restored_completion.is_some()
            || restored.restored_healing
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
                let wire_id = self
                    .normalized_items
                    .get(item_id)
                    .cloned()
                    .ok_or_else(|| error::malformed("persisted OpenAI tool start lost its item"))?;
                if !self.register_call(&wire_id, call_id.clone(), name.clone()) {
                    return Err(error::malformed(
                        "persisted OpenAI tool start contradicted its item",
                    ));
                }
            }
            ModelEvent::TextDelta { item_id, fragment } => {
                let key = self.normalized_parts.get(item_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI text fragment lost its content part")
                })?;
                let part = self.parts.get(&key).ok_or_else(|| {
                    error::malformed("persisted OpenAI text fragment lost its content part")
                })?;
                if !matches!(part.kind, ItemKind::Message | ItemKind::StructuredOutput)
                    || part.completed
                    || part.value_done
                {
                    return Err(error::malformed(
                        "persisted OpenAI text fragment targeted incompatible state",
                    ));
                }
                if part.kind == ItemKind::StructuredOutput {
                    let target = RestoredCompletion::Structured {
                        wire_id: key.0.clone(),
                        content_index: key.1,
                    };
                    self.begin_restored_completion(target)?;
                    self.parts
                        .get_mut(&key)
                        .ok_or_else(|| {
                            error::malformed(
                                "persisted OpenAI text fragment lost its content part",
                            )
                        })?
                        .validated
                        .observe(fragment.expose(), limits.max_output_bytes())?;
                } else {
                    self.parts
                        .get_mut(&key)
                        .ok_or_else(|| {
                            error::malformed(
                                "persisted OpenAI text fragment lost its content part",
                            )
                        })?
                        .observed
                        .observe(fragment.expose(), limits.max_output_bytes())?;
                }
            }
            ModelEvent::StructuredOutputProgress { item_id, revision, fragment } => {
                let key = self.normalized_parts.get(item_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI structured progress lost its content part")
                })?;
                let part = self.parts.get_mut(&key).ok_or_else(|| {
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
                part.observed.observe(fragment.expose(), limits.max_output_bytes())?;
                part.structured
                    .as_mut()
                    .ok_or_else(|| {
                        error::malformed("persisted OpenAI structured buffer was unavailable")
                    })?
                    .append(fragment.expose(), limits)?;
                part.progress_revision = *revision;
            }
            ModelEvent::RefusalDelta { item_id, fragment } => {
                let key = self.normalized_parts.get(item_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI refusal fragment lost its content part")
                })?;
                let part = self.parts.get_mut(&key).ok_or_else(|| {
                    error::malformed("persisted OpenAI refusal fragment lost its content part")
                })?;
                if part.kind != ItemKind::Refusal || part.completed || part.value_done {
                    return Err(error::malformed(
                        "persisted OpenAI refusal fragment targeted incompatible state",
                    ));
                }
                part.observed.observe(fragment.expose(), limits.max_output_bytes())?;
            }
            ModelEvent::ToolArgumentProgress { call_id, revision, fragment } => {
                let wire_id = self.call_items.get(call_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI tool progress lost its call")
                })?;
                let item = self.items.get_mut(&wire_id).ok_or_else(|| {
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
                let wire_id = self.call_items.get(call_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI final tool arguments lost their call")
                })?;
                let item = self.items.get(&wire_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI final tool arguments lost their call")
                })?;
                if item.completed || item.arguments_done {
                    return Err(error::malformed(
                        "persisted OpenAI final tool arguments followed item completion",
                    ));
                }
                self.begin_restored_completion(RestoredCompletion::Tool {
                    wire_id: wire_id.clone(),
                })?;
                self.items
                    .get_mut(&wire_id)
                    .ok_or_else(|| {
                        error::malformed("persisted OpenAI final tool arguments lost their call")
                    })?
                    .final_arguments
                    .get_or_insert_with(ToolArgumentBuffer::new)
                    .append(fragment.expose(), limits)?;
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
            ModelEvent::ProviderEvent(extension)
                if extension.name().as_str() == "openai.state" =>
            {
                let value: Value = serde_json::from_slice(extension.value().canonical_bytes())
                    .map_err(|_| {
                        error::malformed("persisted OpenAI state marker is not valid JSON")
                    })?;
                self.restore_state_marker(&value)?;
            }
            ModelEvent::ProviderEvent(extension)
                if extension.name().as_str() == "peritus.response_healing" =>
            {
                if self.restored_healing {
                    return Err(error::malformed(
                        "persisted OpenAI completion repeated its healing audit",
                    ));
                }
                self.restored_healing = true;
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

    fn begin_restored_completion(
        &mut self,
        target: RestoredCompletion,
    ) -> Result<(), ProviderCoreError> {
        if self
            .restored_completion
            .as_ref()
            .is_some_and(|known| known != &target)
        {
            return Err(error::malformed(
                "persisted OpenAI completion interleaved independent values",
            ));
        }
        self.restored_completion = Some(target);
        Ok(())
    }

    fn finish_restored_frame(&mut self) -> Result<(), ProviderCoreError> {
        if self.pending_item.is_some() || self.pending_part.is_some() {
            return Err(error::malformed(
                "persisted OpenAI coordinate evidence was not followed by its normalized start",
            ));
        }
        let Some(completion) = self.restored_completion.take() else {
            if self.restored_healing {
                return Err(error::malformed(
                    "persisted OpenAI healing audit omitted its completion",
                ));
            }
            return Ok(());
        };
        let healed = core::mem::take(&mut self.restored_healing);
        match completion {
            RestoredCompletion::Structured { wire_id, content_index } => {
                let part = self
                    .parts
                    .get_mut(&(wire_id, content_index))
                    .ok_or_else(|| error::malformed("persisted OpenAI completion lost its part"))?;
                if part.validated.is_empty()
                    || !healed
                        && part.progress_revision > 0
                        && !part.observed.matches(&part.validated)
                {
                    return Err(error::malformed(
                        "persisted OpenAI structured completion contradicted progress",
                    ));
                }
                if part.progress_revision == 0 {
                    part.observed = part.validated.clone();
                }
                part.structured = None;
                part.value_done = true;
            }
            RestoredCompletion::Tool { wire_id } => {
                let item = self.items.get_mut(&wire_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI completion lost its tool item")
                })?;
                let final_arguments = item.final_arguments.take().ok_or_else(|| {
                    error::malformed("persisted OpenAI completion lost final tool arguments")
                })?;
                if !healed
                    && item.argument_progress_revision > 0
                    && item.arguments.as_bytes() != final_arguments.as_bytes()
                {
                    return Err(error::malformed(
                        "persisted OpenAI final tool arguments contradicted progress",
                    ));
                }
                item.arguments = ToolArgumentBuffer::new();
                item.arguments_done = true;
            }
        }
        Ok(())
    }

    fn restore_item_start(
        &mut self,
        item_id: &ItemId,
        index: u32,
        kind: ItemKind,
    ) -> Result<(), ProviderCoreError> {
        if matches!(kind, ItemKind::Message | ItemKind::StructuredOutput | ItemKind::Refusal) {
            let (wire_id, output_index, content_index) = if let Some(pending) = self.pending_part.take()
            {
                let kind_matches = pending.kind == kind
                    || pending.kind == ItemKind::Message
                        && kind == ItemKind::StructuredOutput;
                if !kind_matches {
                    return Err(error::malformed(
                        "persisted OpenAI content start changed ancillary kind",
                    ));
                }
                (pending.wire_id, pending.output_index, pending.content_index)
            } else {
                restore_legacy_content_coordinate(item_id, index)?
            };
            if !self.restore_normalized_index(
                NormalizedCoordinate::Content { output: output_index, content: content_index },
                index,
            ) || !self.claim_normalized_id(item_id.clone())
            {
                return Err(error::malformed(
                    "persisted OpenAI content coordinate or identity collided",
                ));
            }
            if let Some(parent) = self.items.get(&wire_id) {
                if parent.output_index != output_index || parent.kind != ItemKind::Message {
                    return Err(error::malformed(
                        "persisted OpenAI content part contradicted its parent item",
                    ));
                }
            } else {
                let normalized_id = identity::item_id(&wire_id, "")?;
                if !self.insert_item(
                    wire_id.clone(),
                    new_item(normalized_id, output_index, ItemKind::Message),
                ) {
                    return Err(error::malformed(
                        "persisted OpenAI parent item identity collided",
                    ));
                }
            }
            if !self.insert_part(
                wire_id,
                content_index,
                new_part(item_id.clone(), output_index, content_index, kind),
            ) {
                return Err(error::malformed(
                    "persisted OpenAI content part was started more than once",
                ));
            }
            return Ok(());
        }
        let (wire_id, output_index) = if let Some(pending) = self.pending_item.take() {
            if pending.kind != kind {
                return Err(error::malformed(
                    "persisted OpenAI output start changed ancillary kind",
                ));
            }
            (pending.wire_id, pending.output_index)
        } else {
            (item_id.expose_for_wire().to_owned(), index / 65_536)
        };
        if !self.restore_normalized_index(NormalizedCoordinate::Output(output_index), index)
            || !self.claim_normalized_id(item_id.clone())
            || !self.insert_item(
                wire_id,
                new_item(item_id.clone(), output_index, kind),
            )
        {
            return Err(error::malformed(
                "persisted OpenAI output item coordinate or identity collided",
            ));
        }
        Ok(())
    }

    fn restore_item_completed(&mut self, item_id: &ItemId) -> Result<(), ProviderCoreError> {
        if let Some(key) = self.normalized_parts.get(item_id).cloned() {
            let part = self.parts.get_mut(&key).ok_or_else(|| {
                error::malformed("persisted OpenAI content completion lost its part")
            })?;
            if part.completed || !part.value_done {
                return Err(error::malformed(
                    "persisted OpenAI content completion is out of order",
                ));
            }
            part.completed = true;
            if !self.record_part_completion(&key.0) {
                return Err(error::malformed(
                    "persisted OpenAI content completion accounting changed",
                ));
            }
            return Ok(());
        }
        let wire_id = self
            .normalized_items
            .get(item_id)
            .cloned()
            .ok_or_else(|| error::malformed("persisted OpenAI item completion lost its item"))?;
        let item = self
            .items
            .get_mut(&wire_id)
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
            "response.output_item.added" => self.restore_output_item_evidence(value)?,
            "response.content_part.added" => self.restore_content_part_evidence(value)?,
            "response.output_text.done" | "response.refusal.done" => {
                let wire_id = json_str(value, "item_id")?;
                let output_index = json_u32(value, "output_index")?;
                let content_index = json_u32(value, "content_index")?;
                let refusal = kind == "response.refusal.done";
                let complete = json_str_allow_empty(value, if refusal { "refusal" } else { "text" })?;
                let part = self
                    .part_mut(wire_id, content_index)
                    .ok_or_else(|| error::malformed("persisted OpenAI content done lost its part"))?;
                if part.output_index != output_index
                    || part.value_done
                    || refusal != (part.kind == ItemKind::Refusal)
                    || !part.observed.matches_bytes(complete.as_bytes())
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

    fn restore_state_marker(&mut self, value: &Value) -> Result<(), ProviderCoreError> {
        match json_str(value, "type")? {
            "content_done_v1" => {
                let item_id = ItemId::new(json_str(value, "item_id")?.to_owned()).map_err(|_| {
                    error::malformed("persisted OpenAI state item identity is invalid")
                })?;
                let key = self.normalized_parts.get(&item_id).cloned().ok_or_else(|| {
                    error::malformed("persisted OpenAI content state lost its part")
                })?;
                let output_index = json_u32(value, "output_index")?;
                let observed_bytes = json_usize(value, "observed_bytes")?;
                let observed_digest = json_digest(value, "observed_sha256")?;
                let part = self.parts.get_mut(&key).ok_or_else(|| {
                    error::malformed("persisted OpenAI content state lost its part")
                })?;
                if part.output_index != output_index
                    || !matches!(part.kind, ItemKind::Message | ItemKind::Refusal)
                    || part.value_done
                    || part.completed
                    || part.observed.byte_len() != observed_bytes
                    || part.observed.sha256() != observed_digest
                {
                    return Err(error::malformed(
                        "persisted OpenAI content state contradicted its fragments",
                    ));
                }
                part.value_done = true;
            }
            "message_done_v1" => {
                let item_id = ItemId::new(json_str(value, "item_id")?.to_owned()).map_err(|_| {
                    error::malformed("persisted OpenAI state item identity is invalid")
                })?;
                let wire_id = self
                    .normalized_items
                    .get(&item_id)
                    .cloned()
                    .ok_or_else(|| {
                        error::malformed("persisted OpenAI message state lost its item")
                    })?;
                let output_index = json_u32(value, "output_index")?;
                if !self.parts_for_item_complete(&wire_id) {
                    return Err(error::malformed(
                        "persisted OpenAI message ended before its content parts",
                    ));
                }
                let item = self.items.get_mut(&wire_id).ok_or_else(|| {
                    error::malformed("persisted OpenAI message state lost its item")
                })?;
                if item.kind != ItemKind::Message
                    || item.output_index != output_index
                    || item.completed
                {
                    return Err(error::malformed(
                        "persisted OpenAI message state contradicted its item",
                    ));
                }
                item.completed = true;
            }
            _ => {
                return Err(error::malformed(
                    "persisted OpenAI state marker had an unknown type",
                ));
            }
        }
        Ok(())
    }

    fn restore_output_item_evidence(&mut self, value: &Value) -> Result<(), ProviderCoreError> {
        if self.pending_item.is_some() || self.pending_part.is_some() {
            return Err(error::malformed(
                "persisted OpenAI output coordinate evidence overlapped",
            ));
        }
        let output_index = json_u32(value, "output_index")?;
        let wire = json_object(value, "item")?;
        let wire_id = json_str(wire, "id")?.to_owned();
        let kind = item_kind(json_str(wire, "type")?)?;
        if kind == ItemKind::Message {
            let normalized_id = identity::item_id(&wire_id, "")?;
            if !self.insert_item(
                wire_id,
                new_item(normalized_id, output_index, ItemKind::Message),
            ) {
                return Err(error::malformed(
                    "persisted OpenAI message item was added more than once",
                ));
            }
        } else {
            self.pending_item = Some(RestoredItemStart { wire_id, output_index, kind });
        }
        Ok(())
    }

    fn restore_content_part_evidence(&mut self, value: &Value) -> Result<(), ProviderCoreError> {
        if self.pending_item.is_some() || self.pending_part.is_some() {
            return Err(error::malformed(
                "persisted OpenAI content coordinate evidence overlapped",
            ));
        }
        let wire_id = json_str(value, "item_id")?.to_owned();
        let output_index = json_u32(value, "output_index")?;
        let content_index = json_u32(value, "content_index")?;
        let part = json_object(value, "part")?;
        let kind = match json_str(part, "type")? {
            "output_text" => ItemKind::Message,
            "refusal" => ItemKind::Refusal,
            _ => {
                return Err(error::malformed(
                    "persisted OpenAI content coordinate had an unknown kind",
                ));
            }
        };
        self.pending_part = Some(RestoredPartStart {
            wire_id,
            output_index,
            content_index,
            kind,
        });
        Ok(())
    }

    pub fn insert_item(&mut self, wire_id: String, item: ItemState) -> bool {
        if self.items.contains_key(&wire_id)
            || self.output_indexes.contains(&item.output_index)
            || self.normalized_items.contains_key(&item.normalized_id)
            || item
                .call_id
                .as_ref()
                .is_some_and(|call_id| self.call_items.contains_key(call_id))
        {
            return false;
        }
        self.output_indexes.insert(item.output_index);
        self.normalized_items.insert(item.normalized_id.clone(), wire_id.clone());
        if let Some(call_id) = item.call_id.clone() {
            self.call_items.insert(call_id, wire_id.clone());
        }
        let previous = self.items.insert(wire_id, item);
        debug_assert!(previous.is_none());
        true
    }

    pub fn register_call(&mut self, wire_id: &str, call_id: ToolCallId, name: ToolName) -> bool {
        if self.call_items.contains_key(&call_id) {
            return false;
        }
        let Some(item) = self.items.get_mut(wire_id) else { return false };
        if item.kind != ItemKind::ToolCall || item.call_id.is_some() || item.call_name.is_some() {
            return false;
        }
        item.call_id = Some(call_id.clone());
        item.call_name = Some(name);
        self.call_items.insert(call_id, wire_id.to_owned());
        true
    }

    pub fn item(&self, wire_id: &str) -> Option<&ItemState> {
        self.items.get(wire_id)
    }

    pub fn item_mut(&mut self, wire_id: &str) -> Option<&mut ItemState> {
        self.items.get_mut(wire_id)
    }

    pub fn insert_part(&mut self, wire_id: String, content_index: u32, part: PartState) -> bool {
        let key = (wire_id, content_index);
        if self.parts.contains_key(&key) || self.normalized_parts.contains_key(&part.normalized_id) {
            return false;
        }
        let Some(owner) = self.items.get_mut(&key.0) else { return false };
        let Some(parts_started) = owner.parts_started.checked_add(1) else { return false };
        owner.parts_started = parts_started;
        self.normalized_parts.insert(part.normalized_id.clone(), key.clone());
        let previous = self.parts.insert(key, part);
        debug_assert!(previous.is_none());
        true
    }

    pub fn part_mut(&mut self, wire_id: &str, content_index: u32) -> Option<&mut PartState> {
        self.parts.get_mut(&(wire_id.to_owned(), content_index))
    }

    pub fn claim_normalized_id(&mut self, id: ItemId) -> bool {
        self.normalized_ids.insert(id)
    }

    pub fn normalized_index(&mut self, coordinate: NormalizedCoordinate) -> Option<u32> {
        if let Some(index) = self.normalized_coordinates.get(&coordinate) {
            return Some(*index);
        }
        let index = match coordinate
            .legacy_index()
            .filter(|index| !self.normalized_indexes.contains_key(index))
        {
            Some(index) => index,
            None => loop {
                let candidate = self.next_derived_index?;
                self.next_derived_index = candidate.checked_sub(1);
                if !self.normalized_indexes.contains_key(&candidate) {
                    break candidate;
                }
            },
        };
        self.normalized_coordinates.insert(coordinate, index);
        self.normalized_indexes.insert(index, coordinate);
        Some(index)
    }

    fn restore_normalized_index(
        &mut self,
        coordinate: NormalizedCoordinate,
        index: u32,
    ) -> bool {
        if self
            .normalized_coordinates
            .get(&coordinate)
            .is_some_and(|known| *known != index)
            || self
                .normalized_indexes
                .get(&index)
                .is_some_and(|known| *known != coordinate)
        {
            return false;
        }
        self.normalized_coordinates.insert(coordinate, index);
        self.normalized_indexes.insert(index, coordinate);
        if self.next_derived_index == Some(index) {
            self.next_derived_index = index.checked_sub(1);
        }
        true
    }

    pub fn record_part_completion(&mut self, wire_id: &str) -> bool {
        let Some(owner) = self.items.get_mut(wire_id) else { return false };
        if owner.parts_completed >= owner.parts_started {
            return false;
        }
        let Some(parts_completed) = owner.parts_completed.checked_add(1) else { return false };
        owner.parts_completed = parts_completed;
        true
    }

    pub fn parts_for_item_complete(&self, wire_id: &str) -> bool {
        self.items.get(wire_id).is_some_and(|item| {
            item.parts_started != 0 && item.parts_started == item.parts_completed
        })
    }

    pub fn all_items_complete(&self) -> bool {
        self.items.values().all(|item| {
            item.completed && item.parts_started == item.parts_completed
        })
    }

    pub fn has_kind(&self, kind: ItemKind) -> bool {
        self.items.values().any(|item| item.kind == kind)
            || self.parts.values().any(|part| part.kind == kind)
    }
}

fn new_item(normalized_id: ItemId, output_index: u32, kind: ItemKind) -> ItemState {
    ItemState {
        normalized_id,
        output_index,
        kind,
        call_id: None,
        call_name: None,
        arguments: ToolArgumentBuffer::new(),
        final_arguments: None,
        argument_progress_revision: 0,
        arguments_done: false,
        completed: false,
        parts_started: 0,
        parts_completed: 0,
    }
}

fn new_part(
    normalized_id: ItemId,
    output_index: u32,
    content_index: u32,
    kind: ItemKind,
) -> PartState {
    PartState {
        normalized_id,
        output_index,
        content_index,
        kind,
        observed: ObservedValue::new(),
        validated: ObservedValue::new(),
        structured: (kind == ItemKind::StructuredOutput).then(StructuredOutputBuffer::new),
        progress_revision: 0,
        value_done: false,
        completed: false,
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

fn restore_legacy_content_coordinate(
    item_id: &ItemId,
    index: u32,
) -> Result<(String, u32, u32), ProviderCoreError> {
    let normalized = item_id.expose_for_wire();
    if let Some((wire_id, suffix)) = normalized.rsplit_once("-part-")
        && !wire_id.is_empty()
        && let Ok(content_index) = suffix.parse::<u32>()
        && content_index > u16::MAX as u32
        && let Some(remainder) = index.checked_sub(content_index)
        && remainder % 65_536 == 0
    {
        return Ok((wire_id.to_owned(), remainder / 65_536, content_index));
    }
    let output_index = index / 65_536;
    let content_index = index % 65_536;
    Ok((restored_wire_id(item_id, content_index)?, output_index, content_index))
}

fn item_kind(value: &str) -> Result<ItemKind, ProviderCoreError> {
    match value {
        "message" => Ok(ItemKind::Message),
        "function_call" | "custom_tool_call" => Ok(ItemKind::ToolCall),
        "reasoning" => Ok(ItemKind::Reasoning),
        "web_search_call"
        | "file_search_call"
        | "computer_call"
        | "code_interpreter_call"
        | "image_generation_call"
        | "mcp_call"
        | "shell_call" => Ok(ItemKind::ProviderNative),
        _ => Err(error::malformed(
            "persisted OpenAI item coordinate had an unknown kind",
        )),
    }
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

fn json_str_allow_empty<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| error::malformed("persisted OpenAI ancillary string is missing"))
}

fn json_u32(value: &Value, name: &str) -> Result<u32, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|field| u32::try_from(field).ok())
        .ok_or_else(|| error::malformed("persisted OpenAI ancillary index is invalid"))
}

fn json_usize(value: &Value, name: &str) -> Result<usize, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|field| usize::try_from(field).ok())
        .ok_or_else(|| error::malformed("persisted OpenAI state length is invalid"))
}

fn json_digest(value: &Value, name: &str) -> Result<Sha256Digest, ProviderCoreError> {
    let bytes = value
        .get(name)
        .and_then(Value::as_array)
        .filter(|bytes| bytes.len() == Sha256Digest::LENGTH)
        .ok_or_else(|| error::malformed("persisted OpenAI state digest is invalid"))?;
    let mut digest = [0_u8; Sha256Digest::LENGTH];
    for (target, value) in digest.iter_mut().zip(bytes) {
        *target = value
            .as_u64()
            .and_then(|byte| u8::try_from(byte).ok())
            .ok_or_else(|| error::malformed("persisted OpenAI state digest is invalid"))?;
    }
    Ok(Sha256Digest::new(digest))
}

pub(super) enum SequenceDisposition {
    New,
    Duplicate,
    Conflict,
}
