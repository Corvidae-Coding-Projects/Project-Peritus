//! Independent compiled and caller-tightenable F0 bounds.

use crate::{EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery};

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
    /// Maximum supported bounds compiled into schema v1.
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

    /// Constructs caller-selected bounds no larger than the compiled schema ceilings.
    ///
    /// # Errors
    /// Rejects zero fields or any value above its compiled ceiling.
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
        let ceiling = Self::compiled();
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
            || manifests > ceiling.manifests
            || variants > ceiling.variants
            || citations_per_manifest > ceiling.citations_per_manifest
            || deltas_per_manifest > ceiling.deltas_per_manifest
            || predictions_per_manifest > ceiling.predictions_per_manifest
            || attribution_entries > ceiling.attribution_entries
            || criteria > ceiling.criteria
            || text_bytes > ceiling.text_bytes
            || activation_history > ceiling.activation_history
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::LimitExceeded,
                EvolutionOperation::ValidateLimits,
                EvolutionRecovery::ReduceScope,
                "evolution limits are zero or exceed compiled ceilings",
            ));
        }
        Ok(candidate)
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
    /// Maximum independent selection criteria.
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
        Self::compiled()
    }
}
