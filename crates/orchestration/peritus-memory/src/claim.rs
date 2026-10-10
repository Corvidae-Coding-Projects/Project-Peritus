//! Typed derived claims, source provenance, and provider-neutral retrieval features.

use crate::{FeatureKey, FeatureWeight, MemoryError, MemoryErrorKind, MemoryField};

use peritus_codec::sha256;
use peritus_types::Sha256Digest;

#[doc = " Maximum retained memory payload size."]
pub const MAX_MEMORY_CONTENT_BYTES: usize = 65_536;
#[doc = " Maximum token estimate accepted for one memory."]
pub const MAX_MEMORY_TOKENS: u32 = 1_000_000;
#[doc = " Maximum provider-neutral retrieval features on one record or query."]
pub const MAX_RETRIEVAL_FEATURES: usize = 64;
#[doc = " Semantic claim category. A category never grants authority."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClaimType {
    #[doc = " Evidence-backed factual claim."]
    Fact,
    #[doc = " Scoped user or project preference."]
    Preference,
    #[doc = " Previously useful procedure."]
    Procedure,
    #[doc = " Observed outcome."]
    Outcome,
    #[doc = " Risk or failure warning."]
    Warning,
    #[doc = " Derived operational constraint that cannot amend policy."]
    Constraint,
    #[doc = " Claim requiring additional confirmation."]
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
#[doc = " Nonempty canonical set of accepted claim categories."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimTypeSet {
    values: Vec<ClaimType>,
}
impl ClaimTypeSet {
    #[doc = " Creates a nonempty, strictly increasing claim set."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for empty, duplicate, or unordered values."]
    pub fn new(values: Vec<ClaimType>) -> Result<Self, MemoryError> {
        if values.is_empty() {
            return Err(MemoryError::field(MemoryErrorKind::EmptyValue, MemoryField::Features));
        }
        let mut index = 1;
        while index < values.len() {
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
    #[doc = " Returns claim categories in canonical order."]
    #[must_use]
    pub const fn values(&self) -> &[ClaimType] {
        self.values.as_slice()
    }
    #[doc = " Returns whether a category is accepted."]
    #[must_use]
    pub fn contains(&self, claim_type: ClaimType) -> bool {
        let mut index = 0;
        while index < self.values.len() {
            if self.values[index] == claim_type {
                return true;
            }
            index += 1;
        }
        false
    }
}
#[doc = " Original non-authoritative source class retained through memory materialization."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceProvenance {
    #[doc = " Repository files or repository-local instructions."]
    Repository,
    #[doc = " Bounded tool output."]
    Tool,
    #[doc = " Model-provider response data."]
    Provider,
    #[doc = " External network or copied material."]
    External,
    #[doc = " Another agent\'s derived output."]
    Agent,
    #[doc = " Review findings or review evidence."]
    Review,
    #[doc = " User-supplied content retained as quoted evidence."]
    User,
}
impl SourceProvenance {
    #[doc = " Returns the structural non-authority invariant for this closed source enum."]
    #[must_use]
    pub const fn is_non_authoritative(self) -> bool {
        true
    }
}
#[doc = " One provider-neutral retrieval feature."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalFeature {
    key: FeatureKey,
    digest: Sha256Digest,
    weight: FeatureWeight,
}
impl RetrievalFeature {
    #[doc = " Creates a feature from checked constituent values."]
    #[must_use]
    pub const fn new(key: FeatureKey, digest: Sha256Digest, weight: FeatureWeight) -> Self {
        Self { key, digest, weight }
    }
    #[doc = " Returns the stable semantic key."]
    #[must_use]
    pub const fn key(&self) -> FeatureKey {
        self.key
    }
    #[doc = " Returns the exact feature-value digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the bounded feature weight."]
    #[must_use]
    pub const fn weight(&self) -> FeatureWeight {
        self.weight
    }
}
#[doc = " Canonical provider-neutral feature collection."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalFeatures {
    values: Vec<RetrievalFeature>,
}
impl RetrievalFeatures {
    #[doc = " Validates a key-ordered feature collection. Empty collections are valid."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for excessive, duplicate-key, or unordered input."]
    pub fn new(values: Vec<RetrievalFeature>) -> Result<Self, MemoryError> {
        if values.len() > MAX_RETRIEVAL_FEATURES {
            return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Features));
        }
        if values.len() > 1 {
            let mut index = 1;
            while index < values.len() {
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
    #[doc = " Returns an empty feature collection."]
    #[must_use]
    pub const fn empty() -> Self {
        Self { values: Vec::new() }
    }
    #[doc = " Returns features in canonical key order."]
    #[must_use]
    pub const fn values(&self) -> &[RetrievalFeature] {
        self.values.as_slice()
    }
    #[doc = " Returns the feature with a stable key, when present."]
    #[must_use]
    pub fn get(&self, key: FeatureKey) -> Option<&RetrievalFeature> {
        let mut index = 0;
        while index < self.values.len() {
            if self.values[index].key == key {
                return Some(&self.values[index]);
            }
            index += 1;
        }
        None
    }
}
#[doc = " Bounded claim payload retained as inert quoted evidence."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMaterial {
    claim_type: ClaimType,
    digest: Sha256Digest,
    content: Vec<u8>,
    provenance: SourceProvenance,
    estimated_tokens: u32,
}
impl MemoryMaterial {
    #[doc = " Returns the typed claim category."]
    #[must_use]
    pub const fn claim_type(&self) -> ClaimType {
        self.claim_type
    }
    #[doc = " Returns the caller-bound content digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the inert content bytes."]
    #[must_use]
    pub const fn content(&self) -> &[u8] {
        self.content.as_slice()
    }
    #[doc = " Returns the original non-authoritative provenance."]
    #[must_use]
    pub const fn provenance(&self) -> SourceProvenance {
        self.provenance
    }
    #[doc = " Returns the caller-supplied nonzero token estimate."]
    #[must_use]
    pub const fn estimated_tokens(&self) -> u32 {
        self.estimated_tokens
    }
    #[doc = " Memory payloads are always presented as quoted evidence."]
    #[must_use]
    pub const fn quoted_evidence(&self) -> bool {
        self.provenance.is_non_authoritative()
    }
}

// SHA-256 is the narrow hybrid boundary of this H-class crate. All bounds and downstream
// lifecycle/retrieval decisions remain executable Verus code.
impl MemoryMaterial {
    /// Creates checked memory material and verifies exact SHA-256 content binding.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty/oversized content, digest mismatch, or an invalid token
    /// estimate.

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
