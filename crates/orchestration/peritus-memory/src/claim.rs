//! Typed derived claims, source provenance, and provider-neutral retrieval features.

use crate::{FeatureKey, FeatureWeight, MemoryError, MemoryErrorKind, MemoryField};
#[cfg(not(verus_only))]
use peritus_codec::sha256;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Maximum retained memory payload size.
pub const MAX_MEMORY_CONTENT_BYTES: usize = 65_536;
/// Maximum token estimate accepted for one memory.
pub const MAX_MEMORY_TOKENS: u32 = 1_000_000;
/// Maximum provider-neutral retrieval features on one record or query.
pub const MAX_RETRIEVAL_FEATURES: usize = 64;

/// Semantic claim category. A category never grants authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClaimType {
    /// Evidence-backed factual claim.
    Fact,
    /// Scoped user or project preference.
    Preference,
    /// Previously useful procedure.
    Procedure,
    /// Observed outcome.
    Outcome,
    /// Risk or failure warning.
    Warning,
    /// Derived operational constraint that cannot amend policy.
    Constraint,
    /// Claim requiring additional confirmation.
    Hypothesis,
}

impl ClaimType {
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::Fact => 0,
            Self::Preference => 1,
            Self::Procedure => 2,
            Self::Outcome => 3,
            Self::Warning => 4,
            Self::Constraint => 5,
            Self::Hypothesis => 6,
        }
    }
}

/// Nonempty canonical set of accepted claim categories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimTypeSet {
    values: Vec<ClaimType>,
}

impl ClaimTypeSet {
    /// Creates a nonempty, strictly increasing claim set.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty, duplicate, or unordered values.
    pub fn new(values: Vec<ClaimType>) -> Result<Self, MemoryError> {
        if values.is_empty() {
            return Err(MemoryError::field(MemoryErrorKind::EmptyValue, MemoryField::Features));
        }
        let mut index = 1;
        while index < values.len()
            invariant 1 <= index <= values.len(),
            decreases values.len() - index,
        {
            if values[index - 1] == values[index] {
                return Err(MemoryError::field(
                    MemoryErrorKind::DuplicateValue,
                    MemoryField::Features,
                ));
            }
            if values[index - 1].rank() > values[index].rank() {
                return Err(MemoryError::field(
                    MemoryErrorKind::NonCanonicalOrder,
                    MemoryField::Features,
                ));
            }
            index += 1;
        }
        Ok(Self { values })
    }

    /// Returns claim categories in canonical order.
    #[must_use]
    pub const fn values(&self) -> &[ClaimType] { self.values.as_slice() }

    /// Returns whether a category is accepted.
    #[must_use]
    pub fn contains(&self, claim_type: ClaimType) -> bool {
        let mut index = 0;
        while index < self.values.len()
            invariant index <= self.values.len(),
            decreases self.values.len() - index,
        {
            if self.values[index] == claim_type {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// Original non-authoritative source class retained through memory materialization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceProvenance {
    /// Repository files or repository-local instructions.
    Repository,
    /// Bounded tool output.
    Tool,
    /// Model-provider response data.
    Provider,
    /// External network or copied material.
    External,
    /// Another agent's derived output.
    Agent,
    /// Review findings or review evidence.
    Review,
    /// User-supplied content retained as quoted evidence.
    User,
}

impl SourceProvenance {
    /// Every representable memory source is evidence-only and carries no instruction authority.
    pub open spec fn spec_is_non_authoritative(self) -> bool { true }

    /// Returns the structural non-authority invariant for this closed source enum.
    #[must_use]
    pub const fn is_non_authoritative(self) -> (result: bool)
        ensures result == self.spec_is_non_authoritative(),
    {
        true
    }
}

/// One provider-neutral retrieval feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalFeature {
    key: FeatureKey,
    digest: Sha256Digest,
    weight: FeatureWeight,
}

impl RetrievalFeature {
    /// Creates a feature from checked constituent values.
    #[must_use]
    pub const fn new(key: FeatureKey, digest: Sha256Digest, weight: FeatureWeight) -> Self {
        Self { key, digest, weight }
    }

    /// Returns the stable semantic key.
    #[must_use]
    pub const fn key(&self) -> FeatureKey { self.key }

    /// Returns the exact feature-value digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Returns the bounded feature weight.
    #[must_use]
    pub const fn weight(&self) -> FeatureWeight { self.weight }
}

/// Canonical provider-neutral feature collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalFeatures {
    values: Vec<RetrievalFeature>,
}

impl RetrievalFeatures {
    /// Validates a key-ordered feature collection. Empty collections are valid.
    ///
    /// # Errors
    ///
    /// Returns a typed error for excessive, duplicate-key, or unordered input.
    pub fn new(values: Vec<RetrievalFeature>) -> Result<Self, MemoryError> {
        if values.len() > MAX_RETRIEVAL_FEATURES {
            return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Features));
        }
        if values.len() > 1 {
            let mut index = 1;
            while index < values.len()
                invariant 1 <= index <= values.len(),
                decreases values.len() - index,
            {
                if values[index - 1].key == values[index].key {
                    return Err(MemoryError::feature(
                        MemoryErrorKind::DuplicateValue,
                        values[index].key,
                    ));
                }
                if values[index - 1].key > values[index].key {
                    return Err(MemoryError::feature(
                        MemoryErrorKind::NonCanonicalOrder,
                        values[index].key,
                    ));
                }
                index += 1;
            }
        }
        Ok(Self { values })
    }

    /// Returns an empty feature collection.
    #[must_use]
    pub const fn empty() -> Self { Self { values: Vec::new() } }

    /// Returns features in canonical key order.
    #[must_use]
    pub const fn values(&self) -> &[RetrievalFeature] { self.values.as_slice() }

    /// Returns the feature with a stable key, when present.
    #[must_use]
    pub fn get(&self, key: FeatureKey) -> Option<&RetrievalFeature> {
        let mut index = 0;
        while index < self.values.len()
            invariant index <= self.values.len(),
            decreases self.values.len() - index,
        {
            if self.values[index].key == key {
                return Some(&self.values[index]);
            }
            index += 1;
        }
        None
    }
}

/// Bounded claim payload retained as inert quoted evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMaterial {
    claim_type: ClaimType,
    digest: Sha256Digest,
    content: Vec<u8>,
    provenance: SourceProvenance,
    estimated_tokens: u32,
}

impl MemoryMaterial {
    /// Mathematical presentation boundary for every retained payload.
    pub closed spec fn spec_is_quoted_evidence(&self) -> bool {
        self.provenance.spec_is_non_authoritative()
    }

    /// Returns the typed claim category.
    #[must_use]
    pub const fn claim_type(&self) -> ClaimType { self.claim_type }

    /// Returns the caller-bound content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Returns the inert content bytes.
    #[must_use]
    pub const fn content(&self) -> &[u8] { self.content.as_slice() }

    /// Returns the original non-authoritative provenance.
    #[must_use]
    pub const fn provenance(&self) -> SourceProvenance { self.provenance }

    /// Returns the caller-supplied nonzero token estimate.
    #[must_use]
    pub const fn estimated_tokens(&self) -> u32 { self.estimated_tokens }

    /// Memory payloads are always presented as quoted evidence.
    #[must_use]
    pub const fn quoted_evidence(&self) -> (result: bool)
        ensures result == self.spec_is_quoted_evidence(),
    {
        self.provenance.is_non_authoritative()
    }
}

} // verus!

// SHA-256 is the narrow hybrid boundary of this H-class crate. All bounds and downstream
// lifecycle/retrieval decisions remain executable Verus code.
impl MemoryMaterial {
    /// Creates checked memory material and verifies exact SHA-256 content binding.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty/oversized content, digest mismatch, or an invalid token
    /// estimate.
    #[cfg(not(verus_only))]
    pub fn new(
        claim_type: ClaimType,
        digest: Sha256Digest,
        content: Vec<u8>,
        provenance: SourceProvenance,
        estimated_tokens: u32,
    ) -> Result<Self, MemoryError> {
        if content.is_empty() {
            return Err(MemoryError::field(MemoryErrorKind::EmptyValue, MemoryField::Content));
        }
        if content.len() > MAX_MEMORY_CONTENT_BYTES {
            return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Content));
        }
        if sha256(content.as_slice()) != digest {
            return Err(MemoryError::field(MemoryErrorKind::DigestMismatch, MemoryField::Content));
        }
        if estimated_tokens == 0 || estimated_tokens > MAX_MEMORY_TOKENS {
            return Err(MemoryError::field(
                MemoryErrorKind::InvalidBound,
                MemoryField::TokenBudget,
            ));
        }
        Ok(Self { claim_type, digest, content, provenance, estimated_tokens })
    }
}
