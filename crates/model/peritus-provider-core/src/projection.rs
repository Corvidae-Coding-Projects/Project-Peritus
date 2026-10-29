//! Measurements of the bytes an adapter actually rendered, without retaining their content.

use sha2::{Digest as _, Sha256};
mod components;
pub use components::{ProjectionComponent, ProjectionComponentKind, ProjectionComponents};

const BLOCK_BYTES: usize = 4_096;

/// Whether optional efficiency diagnostics are enabled for this host process.
///
/// `PERITUS_EFFICIENCY_LOG=0` or `off` disables diagnostics without changing execution,
/// canonical state, permissions, accounting, request layout, or provider selection.
#[must_use]
pub fn efficiency_diagnostics_enabled() -> bool {
    std::env::var_os("PERITUS_EFFICIENCY_LOG").is_none_or(|value| value != "0" && value != "off")
}

/// Content-free identity of one adapter projection, before vendor-owned framing.
///
/// Parts are hashed in their adapter-defined order. This measures Peritus-rendered bytes;
/// it does not claim vendor submission, token counts, cache acceptance, or native framing.
#[derive(Clone, Debug)]
pub struct RequestProjection {
    layout: &'static str,
    part_bytes: Vec<u64>,
    part_characters: Vec<Option<u64>>,
    components: Option<Vec<ProjectionComponent>>,
    reason: &'static str,
    native_usage: Option<peritus_model_protocol::UsageCounters>,
    native_outcome: Option<&'static str>,
    blocks: Vec<([u8; 32], usize, usize)>,
}

impl RequestProjection {
    /// Measures an already rendered projection without encoding the request a second time.
    #[must_use]
    pub fn measure(layout: &'static str, parts: &[&[u8]]) -> Self {
        let mut blocks = Vec::new();
        for (part_number, mut part) in parts.iter().copied().enumerate() {
            let mut hasher = Sha256::new();
            let mut used = 0;
            while !part.is_empty() {
                let size = (BLOCK_BYTES - used).min(part.len());
                hasher.update(&part[..size]);
                used += size;
                part = &part[size..];
                if used == BLOCK_BYTES {
                    blocks.push((hasher.finalize_reset().into(), used, part_number));
                    used = 0;
                }
            }
            // Include empty parts so a changed origin boundary cannot compare as an
            // identical concatenated string. Part boundaries stay observable.
            blocks.push((hasher.finalize().into(), used, part_number));
        }
        Self {
            layout,
            part_bytes: parts.iter().map(|part| part.len() as u64).collect(),
            part_characters: parts
                .iter()
                .map(|part| std::str::from_utf8(part).ok().map(|text| text.chars().count() as u64))
                .collect(),
            components: None,
            reason: "primary",
            native_usage: None,
            native_outcome: None,
            blocks,
        }
    }

    /// Stable adapter layout revision, including the order and meaning of its parts.
    #[must_use]
    pub const fn layout(&self) -> &'static str {
        self.layout
    }

    /// Exact byte length of each already encoded part, in the declared layout order.
    #[must_use]
    pub fn part_bytes(&self) -> &[u64] {
        &self.part_bytes
    }
    /// Exact Unicode scalar counts, unavailable for non-UTF-8 parts.
    #[must_use]
    pub fn part_characters(&self) -> &[Option<u64>] {
        &self.part_characters
    }
    /// Exact rendered spans attributed by role, block type and fixed host markers.
    #[must_use]
    pub fn components(&self) -> Option<&[ProjectionComponent]> {
        self.components.as_deref()
    }
    /// Adds non-overlapping rendered component spans; unattributed bytes are framing/encoding.
    #[must_use]
    pub fn with_components(mut self, components: &ProjectionComponents) -> Self {
        self.components = components.finish(&self.part_bytes, &self.part_characters);
        self
    }
    /// Distinguishes the primary rendering from an adapter-owned format correction.
    #[must_use]
    pub const fn with_reason(mut self, reason: &'static str) -> Self {
        self.reason = reason;
        self
    }
    /// Diagnostic reason; it does not prove a vendor submission or acceptance.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
    /// Records observed native accounting before logical-request combination, including failures.
    pub const fn observe_native_result(
        &mut self,
        outcome: &'static str,
        usage: Option<peritus_model_protocol::UsageCounters>,
    ) {
        self.native_outcome = Some(outcome);
        self.native_usage = usage;
    }
    /// Native turn outcome when its completed transport output was decoded.
    #[must_use]
    pub const fn native_outcome(&self) -> Option<&'static str> {
        self.native_outcome
    }
    /// Vendor-reported counters for this native turn. Never sum with combined logical usage.
    #[must_use]
    pub const fn native_usage(&self) -> Option<peritus_model_protocol::UsageCounters> {
        self.native_usage
    }

    /// Lower bound on the identical prefix with a preceding projection of the same layout.
    ///
    /// Comparison is in 4096-byte blocks, including a matching final partial block. Hashes
    /// stay private and are never a permission, instruction, or provider-cache identity.
    #[must_use]
    pub fn common_prefix_bytes(&self, previous: &Self) -> u64 {
        if self.layout != previous.layout {
            return 0;
        }
        self.blocks
            .iter()
            .zip(&previous.blocks)
            .take_while(|(left, right)| left == right)
            .map(|(block, _)| block.1 as u64)
            .sum()
    }
}
