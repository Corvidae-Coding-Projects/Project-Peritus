//! Explicit workload policy and independently selected physical checkpoint capacity.

use crate::{EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery};

/// Maximum attribution entries in one independently encoded compatibility page.
///
/// Logical workload policy remains independent: a record may contain any representable number of
/// these pages. Keeping each page within the legacy codec contract lets old single-page bytes stay
/// exact while larger records use the versioned paged representation.
pub(crate) const ATTRIBUTION_PAGE_ENTRIES: usize =
    peritus_codec::CodecLimits::LEGACY_V1.max_collection_items;

/// Converts one logical attribution population to its schema-v1 representable count.
pub(crate) fn represented_attribution_entries(count: usize) -> Option<u32> {
    u32::try_from(count).ok().filter(|count| *count != 0)
}

/// Complete independent logical bounds for one evolution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionLimits {
    manifests: u16,
    variants: u16,
    citations_per_manifest: u16,
    deltas_per_manifest: u16,
    predictions_per_manifest: u16,
    attribution_entries: u32,
    criteria: u16,
    text_bytes: u32,
    activation_history: u16,
}

impl EvolutionLimits {
    /// Frozen former production defaults retained for legacy callers and exact old bytes.
    #[must_use]
    pub const fn compiled() -> Self {
        Self {
            manifests: 256,
            variants: 128,
            citations_per_manifest: 256,
            deltas_per_manifest: 128,
            predictions_per_manifest: 256,
            attribution_entries: 65_536,
            criteria: 64,
            text_bytes: 16_384,
            activation_history: 256,
        }
    }

    /// Workload policy without a cumulative count or text allowance.
    ///
    /// The all-ones field representations are schema-v1 sentinels. Canonical representation
    /// widths and selected checkpoint paging remain independent physical constraints.
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            manifests: u16::MAX,
            variants: u16::MAX,
            citations_per_manifest: u16::MAX,
            deltas_per_manifest: u16::MAX,
            predictions_per_manifest: u16::MAX,
            attribution_entries: u32::MAX,
            criteria: u16::MAX,
            text_bytes: u32::MAX,
            activation_history: u16::MAX,
        }
    }

    /// Constructs an explicit finite or sentinel-unlimited workload policy.
    ///
    /// `criteria` is retained as an inert schema-v1 compatibility field. The closed criterion
    /// catalog and frozen measurement policy determine assessment membership.
    ///
    /// # Errors
    /// Rejects zero fields. All-ones fields select no workload-policy limit.
    #[allow(clippy::too_many_arguments, reason = "independent bounds remain independently visible")]
    pub fn new(
        manifests: u16,
        variants: u16,
        citations_per_manifest: u16,
        deltas_per_manifest: u16,
        predictions_per_manifest: u16,
        attribution_entries: u32,
        criteria: u16,
        text_bytes: u32,
        activation_history: u16,
    ) -> Result<Self, EvolutionError> {
        let candidate = Self {
            manifests,
            variants,
            citations_per_manifest,
            deltas_per_manifest,
            predictions_per_manifest,
            attribution_entries,
            criteria,
            text_bytes,
            activation_history,
        };
        if [
            u64::from(manifests),
            u64::from(variants),
            u64::from(citations_per_manifest),
            u64::from(deltas_per_manifest),
            u64::from(predictions_per_manifest),
            u64::from(attribution_entries),
            u64::from(criteria),
            u64::from(text_bytes),
            u64::from(activation_history),
        ]
        .contains(&0)
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::LimitExceeded,
                EvolutionOperation::ValidateLimits,
                EvolutionRecovery::CorrectInput,
                "evolution workload policy contains a zero allowance",
            ));
        }
        Ok(candidate)
    }

    /// Returns whether this is a strictly broader monotonic successor policy.
    #[must_use]
    pub const fn is_strict_expansion_of(self, predecessor: Self) -> bool {
        self.manifests >= predecessor.manifests
            && self.variants >= predecessor.variants
            && self.citations_per_manifest >= predecessor.citations_per_manifest
            && self.deltas_per_manifest >= predecessor.deltas_per_manifest
            && self.predictions_per_manifest >= predecessor.predictions_per_manifest
            && self.attribution_entries >= predecessor.attribution_entries
            && self.text_bytes >= predecessor.text_bytes
            && self.activation_history >= predecessor.activation_history
            && (self.manifests > predecessor.manifests
                || self.variants > predecessor.variants
                || self.citations_per_manifest > predecessor.citations_per_manifest
                || self.deltas_per_manifest > predecessor.deltas_per_manifest
                || self.predictions_per_manifest > predecessor.predictions_per_manifest
                || self.attribution_entries > predecessor.attribution_entries
                || self.text_bytes > predecessor.text_bytes
                || self.activation_history > predecessor.activation_history)
    }

    /// Maximum manifests retained by one campaign.
    #[must_use]
    pub const fn manifests(self) -> u16 {
        self.manifests
    }
    /// Maximum variants retained by one campaign.
    #[must_use]
    pub const fn variants(self) -> u16 {
        self.variants
    }
    /// Maximum E2 citations in one manifest.
    #[must_use]
    pub const fn citations_per_manifest(self) -> u16 {
        self.citations_per_manifest
    }
    /// Maximum component deltas in one manifest.
    #[must_use]
    pub const fn deltas_per_manifest(self) -> u16 {
        self.deltas_per_manifest
    }
    /// Maximum falsifiable predictions in one manifest.
    #[must_use]
    pub const fn predictions_per_manifest(self) -> u16 {
        self.predictions_per_manifest
    }
    /// Maximum attribution entries in one campaign.
    #[must_use]
    pub const fn attribution_entries(self) -> u32 {
        self.attribution_entries
    }
    /// Returns the inert schema-v1 criteria field retained for exact legacy bytes and digests.
    #[must_use]
    pub const fn criteria(self) -> u16 {
        self.criteria
    }
    /// Maximum bytes in any individual bounded text field.
    #[must_use]
    pub const fn text_bytes(self) -> u32 {
        self.text_bytes
    }
    /// Maximum activation records retained in the pointer checkpoint.
    #[must_use]
    pub const fn activation_history(self) -> u16 {
        self.activation_history
    }

    /// Finite manifest allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn manifests_limit(self) -> Option<u16> {
        finite_u16(self.manifests)
    }
    /// Finite variant allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn variants_limit(self) -> Option<u16> {
        finite_u16(self.variants)
    }
    /// Finite citation allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn citations_per_manifest_limit(self) -> Option<u16> {
        finite_u16(self.citations_per_manifest)
    }
    /// Finite delta allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn deltas_per_manifest_limit(self) -> Option<u16> {
        finite_u16(self.deltas_per_manifest)
    }
    /// Finite prediction allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn predictions_per_manifest_limit(self) -> Option<u16> {
        finite_u16(self.predictions_per_manifest)
    }
    /// Finite attribution allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn attribution_entries_limit(self) -> Option<u32> {
        finite_u32(self.attribution_entries)
    }
    /// Finite text allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn text_bytes_limit(self) -> Option<u32> {
        finite_u32(self.text_bytes)
    }
    /// Finite retained-history allowance, or `None` when policy does not limit it.
    #[must_use]
    pub const fn activation_history_limit(self) -> Option<u16> {
        finite_u16(self.activation_history)
    }

    /// Returns whether a logical attribution population is both representable and within policy.
    #[must_use]
    pub(crate) fn accepts_attribution_entries(self, count: usize) -> bool {
        represented_attribution_entries(count).is_some_and(|count| {
            self.attribution_entries_limit().is_none_or(|maximum| count <= maximum)
        })
    }

    pub(crate) fn digest(self) -> peritus_types::Sha256Digest {
        let mut bytes = Vec::with_capacity(22);
        bytes.extend_from_slice(&self.manifests.to_be_bytes());
        bytes.extend_from_slice(&self.variants.to_be_bytes());
        bytes.extend_from_slice(&self.citations_per_manifest.to_be_bytes());
        bytes.extend_from_slice(&self.deltas_per_manifest.to_be_bytes());
        bytes.extend_from_slice(&self.predictions_per_manifest.to_be_bytes());
        bytes.extend_from_slice(&self.attribution_entries.to_be_bytes());
        bytes.extend_from_slice(&self.criteria.to_be_bytes());
        bytes.extend_from_slice(&self.text_bytes.to_be_bytes());
        bytes.extend_from_slice(&self.activation_history.to_be_bytes());
        crate::identity::digest_parts(b"peritus.f0.evolution-limits.v1\0", &[&bytes])
    }
}

impl Default for EvolutionLimits {
    fn default() -> Self {
        Self::unlimited()
    }
}

const fn finite_u16(value: u16) -> Option<u16> {
    if value == u16::MAX { None } else { Some(value) }
}

const fn finite_u32(value: u32) -> Option<u32> {
    if value == u32::MAX { None } else { Some(value) }
}

/// Physical page capacity selected independently from evolution workload policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionStorageLimits {
    state_page_bytes: usize,
}

impl EvolutionStorageLimits {
    /// Uses the journal's full per-row physical capacity as the default page size.
    #[must_use]
    pub const fn journal_default() -> Self {
        Self { state_page_bytes: peritus_journal::MAX_STATE_BYTES }
    }

    /// Selects the maximum bytes stored in one journal checkpoint page.
    ///
    /// # Errors
    /// Rejects zero or a value larger than the journal's physical state-row capacity.
    pub const fn new(state_page_bytes: usize) -> Result<Self, EvolutionError> {
        if state_page_bytes == 0 || state_page_bytes > peritus_journal::MAX_STATE_BYTES {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::ValidateLimits,
                EvolutionRecovery::CorrectInput,
                "evolution checkpoint page capacity is outside the journal representation",
            ));
        }
        Ok(Self { state_page_bytes })
    }

    /// Maximum bytes in one physical checkpoint page.
    #[must_use]
    pub const fn state_page_bytes(self) -> usize {
        self.state_page_bytes
    }
}

impl Default for EvolutionStorageLimits {
    fn default() -> Self {
        Self::journal_default()
    }
}
