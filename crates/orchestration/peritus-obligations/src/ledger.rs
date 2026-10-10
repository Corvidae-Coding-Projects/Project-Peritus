//! Deterministic extraction of typed requirements from exact public source spans.

mod extraction;

mod validation;

use crate::{ObligationError, ObligationLimits, ObligationSpec, PathMention, RequirementEntry};
use core::cmp::Ordering;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;

#[doc = " One proposed typed extraction retaining its exact public byte span."]
#[derive(Debug, Eq, PartialEq)]
pub struct RequirementDraft {
    id: RequirementId,
    byte_start: usize,
    byte_end: usize,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
}
impl RequirementDraft {
    #[doc = " Creates an inert extraction proposal. [`RequirementLedger::extract`] validates the span,"]
    #[doc = " ordering, typed details, paths, and alternative topology against the public source."]
    #[must_use]
    pub const fn new(
        id: RequirementId,
        byte_start: usize,
        byte_end: usize,
        specification: ObligationSpec,
        paths: Vec<PathMention>,
    ) -> Self {
        Self { id, byte_start, byte_end, specification, paths }
    }
    #[doc = " Stable requirement identity."]
    #[must_use]
    pub const fn id(&self) -> RequirementId {
        self.id
    }
    #[doc = " Inclusive public-source byte offset."]
    #[must_use]
    pub const fn byte_start(&self) -> usize {
        self.byte_start
    }
    #[doc = " Exclusive public-source byte offset."]
    #[must_use]
    pub const fn byte_end(&self) -> usize {
        self.byte_end
    }
    #[doc = " Proposed typed requirement."]
    #[must_use]
    pub const fn specification(&self) -> &ObligationSpec {
        &self.specification
    }
    #[doc = " Exact classified path mentions."]
    #[must_use]
    pub const fn paths(&self) -> &[PathMention] {
        self.paths.as_slice()
    }
}
impl Clone for RequirementDraft {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            byte_start: self.byte_start,
            byte_end: self.byte_end,
            specification: self.specification.clone(),
            paths: PathMention::clone_sequence(self.paths.as_slice()),
        }
    }
}
#[doc = " Canonical exact-clause requirement ledger for one public task source."]
#[derive(Debug, Eq, PartialEq)]
pub struct RequirementLedger {
    source_digest: Sha256Digest,
    conversation_revision: u64,
    digest: Sha256Digest,
    entries: Vec<RequirementEntry>,
    limits: ObligationLimits,
}
impl RequirementLedger {
    pub(crate) fn extract_preimage(
        source: &crate::PublicTaskSource,
        drafts: &[RequirementDraft],
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        validation::validate_drafts(source, drafts, limits)?;
        extraction::build(source, drafts, limits)
    }
    #[doc = " Complete public source digest supplied at the ordinary source boundary."]
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    #[doc = " Public conversation revision."]
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }
    #[doc = " Digest returned by the ordinary canonical SHA-256 boundary."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Requirements in canonical identity order."]
    #[must_use]
    pub const fn entries(&self) -> &[RequirementEntry] {
        self.entries.as_slice()
    }
    #[doc = " Bounds used for this ledger."]
    #[must_use]
    pub const fn limits(&self) -> ObligationLimits {
        self.limits
    }
    #[doc = " Finds the unique requirement with the exact stable identity."]
    #[must_use]
    pub fn entry(&self, id: RequirementId) -> Option<&RequirementEntry> {
        let mut low = 0usize;
        let mut high = self.entries.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let order = crate::order::compare(
                self.entries[middle].id().digest().as_bytes(),
                id.digest().as_bytes(),
            );
            match order {
                Ordering::Equal => {
                    let entry = &self.entries[middle];
                    return Some(entry);
                }
                Ordering::Less => {
                    low = middle + 1;
                }
                Ordering::Greater => {
                    high = middle;
                }
            }
        }
        None
    }
}
impl Clone for RequirementLedger {
    fn clone(&self) -> Self {
        let entries = RequirementEntry::clone_sequence(self.entries.as_slice());
        Self {
            source_digest: self.source_digest,
            conversation_revision: self.conversation_revision,
            digest: self.digest,
            entries,
            limits: self.limits,
        }
    }
}

impl RequirementLedger {
    /// Validates proposals, copies exact source clauses, and hashes the canonical ledger preimage.
    ///
    /// The verified extraction kernel binds every preimage field, and the verified canonical
    /// encoder supplies the exact deterministic bytes. SHA-256 execution remains an ordinary
    /// cryptographic boundary and receives no proof axiom.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized collections, unordered or duplicate identities, invalid source
    /// spans, incompatible typed details, noncanonical paths, and alternative groups with fewer
    /// than two distinct branches.
    pub fn extract(
        source: &crate::PublicTaskSource,
        drafts: Vec<RequirementDraft>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        let result = Self::extract_preimage(source, drafts.as_slice(), limits);
        drop(drafts);
        let mut ledger = result?;
        ledger.digest =
            crate::canonical::sha256(crate::canonical::encode::ledger_bytes(&ledger).as_slice());
        Ok(ledger)
    }
}
