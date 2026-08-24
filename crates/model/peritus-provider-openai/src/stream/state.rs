//! Private item/content assembly and provider sequence state.

use std::collections::BTreeMap;

use peritus_model_protocol::{ItemId, ItemKind, ResponseId, ToolCallId, ToolName};
use peritus_types::Sha256Digest;

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
    pub arguments: Vec<u8>,
    pub arguments_done: bool,
    pub completed: bool,
}

pub(super) struct PartState {
    pub normalized_id: ItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub kind: ItemKind,
    pub bytes: Vec<u8>,
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

pub(super) enum SequenceDisposition {
    New,
    Duplicate,
    Conflict,
}
