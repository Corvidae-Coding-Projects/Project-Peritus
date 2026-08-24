use std::collections::BTreeMap;

use peritus_model_protocol::{ItemId, ItemKind, ResponseId, ToolCallId};
use peritus_types::Sha256Digest;

pub(super) struct ResponsesState {
    response_id: Option<ResponseId>,
    started: bool,
    last_sequence: Option<u64>,
    seen: BTreeMap<u64, Sha256Digest>,
    items: BTreeMap<String, ItemState>,
    parts: BTreeMap<(String, u32), PartState>,
}

pub(super) struct ItemState {
    pub normalized: ItemId,
    pub index: u32,
    pub kind: ItemKind,
    pub call_id: Option<ToolCallId>,
    pub bytes: Vec<u8>,
    pub value_done: bool,
    pub completed: bool,
}

pub(super) struct PartState {
    pub normalized: ItemId,
    pub index: u32,
    pub kind: ItemKind,
    pub bytes: Vec<u8>,
    pub value_done: bool,
    pub completed: bool,
}

impl ResponsesState {
    pub const fn new() -> Self {
        Self {
            response_id: None,
            started: false,
            last_sequence: None,
            seen: BTreeMap::new(),
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

    pub fn start(&mut self, id: ResponseId) -> bool {
        if self.started {
            return false;
        }
        self.started = true;
        self.response_id = Some(id);
        true
    }

    pub fn response_matches(&self, id: &str) -> bool {
        self.response_id.as_ref().is_some_and(|value| value.expose_for_wire() == id)
    }

    pub fn sequence(&mut self, value: u64, digest: Sha256Digest) -> SequenceDisposition {
        if let Some(known) = self.seen.get(&value) {
            return if *known == digest {
                SequenceDisposition::Duplicate
            } else {
                SequenceDisposition::Conflict
            };
        }
        if value == 0 || self.last_sequence.is_some_and(|last| value <= last) {
            return SequenceDisposition::Conflict;
        }
        self.last_sequence = Some(value);
        self.seen.insert(value, digest);
        SequenceDisposition::New
    }

    pub fn insert_item(&mut self, id: String, item: ItemState) -> bool {
        self.items.insert(id, item).is_none()
    }

    pub fn item(&self, id: &str) -> Option<&ItemState> {
        self.items.get(id)
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut ItemState> {
        self.items.get_mut(id)
    }

    pub fn insert_part(&mut self, item: String, content: u32, part: PartState) -> bool {
        self.parts.insert((item, content), part).is_none()
    }

    pub fn part_mut(&mut self, item: &str, content: u32) -> Option<&mut PartState> {
        self.parts.get_mut(&(item.to_owned(), content))
    }

    pub fn parts_complete(&self, item: &str) -> bool {
        let mut found = false;
        for (_, part) in self.parts.iter().filter(|((id, _), _)| id == item) {
            found = true;
            if !part.completed {
                return false;
            }
        }
        found
    }

    pub fn all_complete(&self) -> bool {
        self.items.values().all(|value| value.completed)
            && self.parts.values().all(|value| value.completed)
    }

    pub fn has_kind(&self, kind: ItemKind) -> bool {
        self.items.values().any(|value| value.kind == kind)
            || self.parts.values().any(|value| value.kind == kind)
    }
}

pub(super) enum SequenceDisposition {
    New,
    Duplicate,
    Conflict,
}
