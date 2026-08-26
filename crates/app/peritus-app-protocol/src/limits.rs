//! Explicit resource limits for application-protocol messages and pure state machines.

use core::fmt;
use peritus_codec::{CodecLimits, HEADER_LEN};
use vstd::prelude::*;

verus! {

/// Mathematical validity predicate for one positive bounded resource pair.
pub open spec fn valid_bounded_resource(value: int, ceiling: int) -> bool {
    0 < value && value <= ceiling
}

/// The minimum of two positive limits is positive.
pub proof fn minimum_positive(left: int, right: int)
    requires left > 0, right > 0
    ensures if left <= right { left } else { right } > 0
{
}

} // verus!

/// Checked resource ceilings used by one protocol relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AppProtocolLimits {
    codec: CodecLimits,
    max_versions: usize,
    max_features: usize,
    max_idempotency_entries: usize,
    max_topics: usize,
    max_in_flight_events: usize,
    max_artifact_chunk_bytes: usize,
    max_prompt_choices: usize,
    max_terminal_chunk_bytes: usize,
    max_diagnostic_bytes: usize,
    max_remaining_work_items: usize,
}

impl AppProtocolLimits {
    /// Production version-one resource ceilings.
    pub const PRODUCTION: Self = Self {
        codec: CodecLimits::PRODUCTION,
        max_versions: 16,
        max_features: 64,
        max_idempotency_entries: 4_096,
        max_topics: 64,
        max_in_flight_events: 256,
        max_artifact_chunk_bytes: 256 * 1024,
        max_prompt_choices: 64,
        max_terminal_chunk_bytes: 64 * 1024,
        max_diagnostic_bytes: 4 * 1024,
        max_remaining_work_items: 256,
    };

    /// Creates and validates independent application and codec ceilings.
    ///
    /// # Errors
    ///
    /// Returns [`LimitConfigurationError`] when a ceiling is zero or cannot be represented safely
    /// within its enclosing codec ceiling.
    #[allow(clippy::too_many_arguments, reason = "independent protocol limits are security inputs")]
    pub fn new(
        codec: CodecLimits,
        max_versions: usize,
        max_features: usize,
        max_idempotency_entries: usize,
        max_topics: usize,
        max_in_flight_events: usize,
        max_artifact_chunk_bytes: usize,
        max_prompt_choices: usize,
        max_terminal_chunk_bytes: usize,
        max_diagnostic_bytes: usize,
        max_remaining_work_items: usize,
    ) -> Result<Self, LimitConfigurationError> {
        let candidate = Self {
            codec,
            max_versions,
            max_features,
            max_idempotency_entries,
            max_topics,
            max_in_flight_events,
            max_artifact_chunk_bytes,
            max_prompt_choices,
            max_terminal_chunk_bytes,
            max_diagnostic_bytes,
            max_remaining_work_items,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    /// Computes the pointwise minimum accepted by both peers.
    ///
    /// # Errors
    ///
    /// Returns an error only if either input was constructed outside this type's invariants.
    pub fn negotiated(self, other: Self) -> Result<Self, LimitConfigurationError> {
        let codec = CodecLimits::new(
            self.codec.max_frame_bytes.min(other.codec.max_frame_bytes),
            self.codec.max_payload_bytes.min(other.codec.max_payload_bytes),
            self.codec.max_collection_items.min(other.codec.max_collection_items),
            self.codec.max_string_bytes.min(other.codec.max_string_bytes),
            self.codec.max_opaque_bytes.min(other.codec.max_opaque_bytes),
            self.codec.max_nesting_depth.min(other.codec.max_nesting_depth),
        );
        Self::new(
            codec,
            self.max_versions.min(other.max_versions),
            self.max_features.min(other.max_features),
            self.max_idempotency_entries.min(other.max_idempotency_entries),
            self.max_topics.min(other.max_topics),
            self.max_in_flight_events.min(other.max_in_flight_events),
            self.max_artifact_chunk_bytes.min(other.max_artifact_chunk_bytes),
            self.max_prompt_choices.min(other.max_prompt_choices),
            self.max_terminal_chunk_bytes.min(other.max_terminal_chunk_bytes),
            self.max_diagnostic_bytes.min(other.max_diagnostic_bytes),
            self.max_remaining_work_items.min(other.max_remaining_work_items),
        )
    }

    /// Returns whether this limit set is at least as permissive as `requested` in every dimension.
    #[must_use]
    pub const fn permits_all(self, requested: Self) -> bool {
        self.codec.max_frame_bytes >= requested.codec.max_frame_bytes
            && self.codec.max_payload_bytes >= requested.codec.max_payload_bytes
            && self.codec.max_collection_items >= requested.codec.max_collection_items
            && self.codec.max_string_bytes >= requested.codec.max_string_bytes
            && self.codec.max_opaque_bytes >= requested.codec.max_opaque_bytes
            && self.codec.max_nesting_depth >= requested.codec.max_nesting_depth
            && self.max_versions >= requested.max_versions
            && self.max_features >= requested.max_features
            && self.max_idempotency_entries >= requested.max_idempotency_entries
            && self.max_topics >= requested.max_topics
            && self.max_in_flight_events >= requested.max_in_flight_events
            && self.max_artifact_chunk_bytes >= requested.max_artifact_chunk_bytes
            && self.max_prompt_choices >= requested.max_prompt_choices
            && self.max_terminal_chunk_bytes >= requested.max_terminal_chunk_bytes
            && self.max_diagnostic_bytes >= requested.max_diagnostic_bytes
            && self.max_remaining_work_items >= requested.max_remaining_work_items
    }

    /// Returns the nested canonical-codec ceilings.
    #[must_use]
    pub const fn codec(self) -> CodecLimits {
        self.codec
    }
    /// Returns the maximum advertised version ranges.
    #[must_use]
    pub const fn max_versions(self) -> usize {
        self.max_versions
    }
    /// Returns the maximum feature names in one collection.
    #[must_use]
    pub const fn max_features(self) -> usize {
        self.max_features
    }
    /// Returns the maximum retained final idempotency entries.
    #[must_use]
    pub const fn max_idempotency_entries(self) -> usize {
        self.max_idempotency_entries
    }
    /// Returns the maximum subscription topics.
    #[must_use]
    pub const fn max_topics(self) -> usize {
        self.max_topics
    }
    /// Returns the maximum unacknowledged event deliveries.
    #[must_use]
    pub const fn max_in_flight_events(self) -> usize {
        self.max_in_flight_events
    }
    /// Returns the maximum artifact chunk bytes.
    #[must_use]
    pub const fn max_artifact_chunk_bytes(self) -> usize {
        self.max_artifact_chunk_bytes
    }
    /// Returns the maximum choices in one prompt.
    #[must_use]
    pub const fn max_prompt_choices(self) -> usize {
        self.max_prompt_choices
    }
    /// Returns the maximum terminal-stream chunk bytes.
    #[must_use]
    pub const fn max_terminal_chunk_bytes(self) -> usize {
        self.max_terminal_chunk_bytes
    }
    /// Returns the maximum diagnostic UTF-8 bytes.
    #[must_use]
    pub const fn max_diagnostic_bytes(self) -> usize {
        self.max_diagnostic_bytes
    }
    /// Returns the maximum bounded remaining-work records.
    #[must_use]
    pub const fn max_remaining_work_items(self) -> usize {
        self.max_remaining_work_items
    }

    fn validate(self) -> Result<(), LimitConfigurationError> {
        let codec = self.codec;
        positive(codec.max_frame_bytes, LimitDimension::FrameBytes)?;
        positive(codec.max_payload_bytes, LimitDimension::PayloadBytes)?;
        positive(codec.max_collection_items, LimitDimension::CollectionItems)?;
        positive(codec.max_string_bytes, LimitDimension::StringBytes)?;
        positive(codec.max_opaque_bytes, LimitDimension::OpaqueBytes)?;
        if codec.max_nesting_depth == 0 {
            return Err(LimitConfigurationError::Zero(LimitDimension::NestingDepth));
        }
        let payload_capacity = codec
            .max_frame_bytes
            .checked_sub(HEADER_LEN)
            .ok_or(LimitConfigurationError::Inconsistent(LimitDimension::PayloadBytes))?;
        bounded(codec.max_payload_bytes, payload_capacity, LimitDimension::PayloadBytes)?;
        bounded(codec.max_string_bytes, codec.max_payload_bytes, LimitDimension::StringBytes)?;
        bounded(codec.max_opaque_bytes, codec.max_payload_bytes, LimitDimension::OpaqueBytes)?;
        for (value, dimension) in [
            (self.max_versions, LimitDimension::Versions),
            (self.max_features, LimitDimension::Features),
            (self.max_idempotency_entries, LimitDimension::IdempotencyEntries),
            (self.max_topics, LimitDimension::Topics),
            (self.max_in_flight_events, LimitDimension::InFlightEvents),
            (self.max_prompt_choices, LimitDimension::PromptChoices),
            (self.max_remaining_work_items, LimitDimension::RemainingWorkItems),
        ] {
            bounded(value, codec.max_collection_items, dimension)?;
        }
        bounded(
            self.max_artifact_chunk_bytes,
            codec.max_opaque_bytes,
            LimitDimension::ArtifactChunkBytes,
        )?;
        bounded(
            self.max_terminal_chunk_bytes,
            codec.max_opaque_bytes,
            LimitDimension::TerminalChunkBytes,
        )?;
        bounded(self.max_diagnostic_bytes, codec.max_string_bytes, LimitDimension::DiagnosticBytes)
    }
}

impl Default for AppProtocolLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

const fn positive(value: usize, dimension: LimitDimension) -> Result<(), LimitConfigurationError> {
    if value == 0 { Err(LimitConfigurationError::Zero(dimension)) } else { Ok(()) }
}

fn bounded(
    value: usize,
    ceiling: usize,
    dimension: LimitDimension,
) -> Result<(), LimitConfigurationError> {
    positive(value, dimension)?;
    if value > ceiling { Err(LimitConfigurationError::Inconsistent(dimension)) } else { Ok(()) }
}

/// Stable dimension named by a limit-configuration failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LimitDimension {
    /// Complete frame bytes.
    FrameBytes,
    /// Frame payload bytes.
    PayloadBytes,
    /// Collection items.
    CollectionItems,
    /// UTF-8 string bytes.
    StringBytes,
    /// Opaque field bytes.
    OpaqueBytes,
    /// Aggregate nesting depth.
    NestingDepth,
    /// Version ranges.
    Versions,
    /// Feature names.
    Features,
    /// Retained idempotency results.
    IdempotencyEntries,
    /// Subscription topics.
    Topics,
    /// Unacknowledged event deliveries.
    InFlightEvents,
    /// Artifact chunk bytes.
    ArtifactChunkBytes,
    /// Prompt choices.
    PromptChoices,
    /// Terminal chunk bytes.
    TerminalChunkBytes,
    /// Diagnostic bytes.
    DiagnosticBytes,
    /// Remaining-work records.
    RemainingWorkItems,
}

/// Invalid application-protocol limit configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LimitConfigurationError {
    /// One required ceiling was zero.
    Zero(LimitDimension),
    /// One ceiling exceeded the enclosing resource ceiling.
    Inconsistent(LimitDimension),
}

impl fmt::Display for LimitConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero(dimension) => write!(formatter, "{dimension:?} limit must be positive"),
            Self::Inconsistent(dimension) => {
                write!(formatter, "{dimension:?} limit exceeds its enclosing ceiling")
            }
        }
    }
}

impl std::error::Error for LimitConfigurationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_codec_relationship_is_rejected() {
        let codec = CodecLimits::new(HEADER_LEN, 1, 1, 1, 1, 1);
        let result = AppProtocolLimits::new(codec, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1);
        assert_eq!(
            result,
            Err(LimitConfigurationError::Inconsistent(LimitDimension::PayloadBytes)),
        );
    }

    #[test]
    fn negotiation_is_pointwise_minimum() {
        let constrained =
            AppProtocolLimits::new(CodecLimits::PRODUCTION, 2, 3, 4, 5, 6, 1024, 7, 512, 128, 8)
                .unwrap();
        assert_eq!(AppProtocolLimits::PRODUCTION.negotiated(constrained), Ok(constrained),);
    }
}
