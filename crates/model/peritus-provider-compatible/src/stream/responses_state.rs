use std::collections::{BTreeMap, BTreeSet};

use peritus_model_protocol::{ItemId, ItemKind, ResponseId, ToolCallId};
use peritus_types::Sha256Digest;

pub(super) struct ResponsesState {
    response_id: Option<ResponseId>,
    started: bool,
    last_sequence: Option<u64>,
    seen: BTreeMap<u64, Sha256Digest>,
    items: BTreeMap<String, ItemState>,
    parts: BTreeMap<(String, u32), PartState>,
    output_indexes: BTreeSet<u32>,
    normalized_ids: BTreeSet<ItemId>,
    normalized_coordinates: BTreeMap<NormalizedCoordinate, u32>,
    normalized_indexes: BTreeMap<u32, NormalizedCoordinate>,
    next_derived_index: Option<u32>,
}

pub(super) struct ItemState {
    pub normalized: ItemId,
    pub index: u32,
    pub kind: ItemKind,
    pub call_id: Option<ToolCallId>,
    pub bytes: peritus_provider_core::healing::ToolArgumentBuffer,
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum NormalizedCoordinate {
    Output(u32),
    Content { output: u32, content: u32 },
}

impl NormalizedCoordinate {
    const fn legacy_index(self) -> Option<u32> {
        match self {
            Self::Output(output) => Some(output),
            Self::Content { output, content }
                if output <= u16::MAX as u32 && content <= u16::MAX as u32 =>
            {
                Some((output << 16) | content)
            }
            Self::Content { .. } => None,
        }
    }
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
            output_indexes: BTreeSet::new(),
            normalized_ids: BTreeSet::new(),
            normalized_coordinates: BTreeMap::new(),
            normalized_indexes: BTreeMap::new(),
            next_derived_index: Some(u32::MAX),
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
        if self.items.contains_key(&id) || self.output_indexes.contains(&item.index) {
            return false;
        }
        self.output_indexes.insert(item.index);
        let previous = self.items.insert(id, item);
        debug_assert!(previous.is_none());
        true
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
        let previous_coordinate = self.normalized_coordinates.insert(coordinate, index);
        let previous_index = self.normalized_indexes.insert(index, coordinate);
        debug_assert!(previous_coordinate.is_none() && previous_index.is_none());
        Some(index)
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
