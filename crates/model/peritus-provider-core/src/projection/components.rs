//! Content-free attribution of disjoint spans in already rendered adapter buffers.

use peritus_model_protocol::{ContentBlock, Message, Role};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

/// Diagnostic attribution only; this never changes instruction or evidence authority.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum ProjectionComponentKind {
    /// Host system/developer policy blocks.
    Policy,
    /// Rendered tool catalog and response grammar.
    Schemas,
    /// Host tool-result blocks and explicitly marked exact evidence cards.
    ExactSource,
    /// Other conversation and mixed-content bodies, without semantic inference.
    Conversation,
    /// Explicitly marked local working-state blocks.
    DerivedState,
    /// Explicitly marked scope and pending/live-control blocks.
    LiveControl,
    /// Remaining delimiters, envelopes, encoding and transport instructions.
    Framing,
}
impl ProjectionComponentKind {
    /// Classifies the existing block's role/type and fixed host marker; mixed bodies stay mixed.
    #[must_use]
    pub fn block(role: Role, block: &ContentBlock) -> Self {
        if let ContentBlock::Text(text) = block {
            let text = text.expose_for_wire();
            if text.starts_with("LOCAL WORKING STATE") {
                return Self::DerivedState;
            }
            if text.starts_with("Local context scope=")
                || text.starts_with("HOST_PENDING")
                || text.starts_with("CURRENT HOST INVOCATION STATE")
            {
                return Self::LiveControl;
            }
            if text.starts_with("DECISION TOOL EVIDENCE") || text.starts_with("VERIFIED SOURCE") {
                return Self::ExactSource;
            }
        }
        match (role, block) {
            (_, ContentBlock::ToolResult(_)) => Self::ExactSource,
            (Role::System | Role::Developer, _) => Self::Policy,
            _ => Self::Conversation,
        }
    }
    /// Stable content-free category name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Schemas => "schemas",
            Self::ExactSource => "exact_source",
            Self::Conversation => "conversation_or_mixed",
            Self::DerivedState => "derived_state",
            Self::LiveControl => "live_control",
            Self::Framing => "framing_and_encoding",
        }
    }
}
const KINDS: [ProjectionComponentKind; 7] = [
    ProjectionComponentKind::Policy,
    ProjectionComponentKind::Schemas,
    ProjectionComponentKind::ExactSource,
    ProjectionComponentKind::Conversation,
    ProjectionComponentKind::DerivedState,
    ProjectionComponentKind::LiveControl,
    ProjectionComponentKind::Framing,
];

/// One attributed contribution to the actual final rendered parts.
#[derive(Clone, Debug)]
pub struct ProjectionComponent {
    kind: ProjectionComponentKind,
    bytes: u64,
    characters: u64,
}
impl ProjectionComponent {
    /// Stable diagnostic category.
    #[must_use]
    pub const fn kind(&self) -> ProjectionComponentKind {
        self.kind
    }
    /// Exact rendered byte contribution.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Exact rendered Unicode scalar contribution.
    #[must_use]
    pub const fn characters(&self) -> u64 {
        self.characters
    }
}

/// Counts spans without retaining text or serializing a request again.
#[derive(Clone, Debug, Default)]
pub struct ProjectionComponents {
    bytes: [u64; 7],
    characters: [u64; 7],
    unavailable: bool,
}
impl ProjectionComponents {
    /// Adds a disjoint actual encoded span. Overlaps or invalid input make attribution unavailable.
    pub fn add(&mut self, kind: ProjectionComponentKind, bytes: &[u8]) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.unavailable = true;
            return;
        };
        let index = kind as usize;
        let size = self.bytes[index].checked_add(bytes.len() as u64);
        let chars = self.characters[index].checked_add(text.chars().count() as u64);
        match (size, chars) {
            (Some(size), Some(chars)) => {
                self.bytes[index] = size;
                self.characters[index] = chars;
            }
            _ => self.unavailable = true,
        }
    }
    /// Reads borrowed spans of an already encoded JSON message; no body is copied or re-encoded.
    pub fn add_json_message(&mut self, original: &Message, encoded: &str) {
        let parsed = (|| -> Option<()> {
            let fields: BTreeMap<&str, &RawValue> = serde_json::from_str(encoded).ok()?;
            let blocks: Vec<&RawValue> = serde_json::from_str(fields.get("content")?.get()).ok()?;
            if blocks.len() != original.content().len() {
                return None;
            }
            for (block, raw) in original.content().iter().zip(blocks) {
                self.add(
                    ProjectionComponentKind::block(original.role(), block),
                    raw.get().as_bytes(),
                );
            }
            Some(())
        })();
        self.unavailable |= parsed.is_none();
    }
    pub(super) fn finish(
        &self,
        parts: &[u64],
        chars: &[Option<u64>],
    ) -> Option<Vec<ProjectionComponent>> {
        if self.unavailable {
            return None;
        }
        let total = parts.iter().try_fold(0u64, |sum, n| sum.checked_add(*n))?;
        let total_chars = chars.iter().try_fold(0u64, |sum, n| sum.checked_add((*n)?))?;
        let attributed = self.bytes.iter().try_fold(0u64, |sum, n| sum.checked_add(*n))?;
        let attributed_chars =
            self.characters.iter().try_fold(0u64, |sum, n| sum.checked_add(*n))?;
        let remaining = total.checked_sub(attributed)?;
        let remaining_chars = total_chars.checked_sub(attributed_chars)?;
        Some(
            KINDS
                .iter()
                .enumerate()
                .map(|(index, kind)| ProjectionComponent {
                    kind: *kind,
                    bytes: self.bytes[index] + if index == 6 { remaining } else { 0 },
                    characters: self.characters[index]
                        + if index == 6 { remaining_chars } else { 0 },
                })
                .collect(),
        )
    }
}
