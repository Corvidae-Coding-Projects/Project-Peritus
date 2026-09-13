//! Deterministic extraction of typed requirements from exact public source spans.

#[cfg(verus_only)]
mod admission;
mod extraction;
#[cfg(verus_only)]
mod model;
mod validation;

use crate::{ObligationError, ObligationLimits, ObligationSpec, PathMention, RequirementEntry};
use core::cmp::Ordering;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// One proposed typed extraction retaining its exact public byte span.
#[derive(Debug, Eq, PartialEq)]
pub struct RequirementDraft {
    id: RequirementId,
    byte_start: usize,
    byte_end: usize,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
}

impl RequirementDraft {
    /// Exact proposed requirement identity.
    pub closed spec fn spec_id(&self) -> RequirementId { self.id }
    /// Exact inclusive public-source offset.
    pub closed spec fn spec_byte_start(&self) -> usize { self.byte_start }
    /// Exact exclusive public-source offset.
    pub closed spec fn spec_byte_end(&self) -> usize { self.byte_end }
    /// Exact proposed typed requirement.
    pub closed spec fn spec_specification(&self) -> ObligationSpec { self.specification }
    /// Exact proposed path sequence.
    pub closed spec fn spec_paths(&self) -> Seq<PathMention> { self.paths@ }

    /// Complete semantic equality of a draft.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        &&& model::requirement_key(self.spec_id()) == model::requirement_key(other.spec_id())
        &&& self.spec_byte_start() == other.spec_byte_start()
        &&& self.spec_byte_end() == other.spec_byte_end()
        &&& self.spec_specification().spec_same_content(&other.spec_specification())
        &&& PathMention::sequence_same_content(self.spec_paths(), other.spec_paths())
    }

    /// Creates an inert extraction proposal. [`RequirementLedger::extract`] validates the span,
    /// ordering, typed details, paths, and alternative topology against the public source.
    #[must_use]
    pub const fn new(
        id: RequirementId,
        byte_start: usize,
        byte_end: usize,
        specification: ObligationSpec,
        paths: Vec<PathMention>,
    ) -> (value: Self)
        ensures value.spec_id() == id,
            value.spec_byte_start() == byte_start,
            value.spec_byte_end() == byte_end,
            value.spec_specification() == specification,
            value.spec_paths() == paths@,
    {
        Self { id, byte_start, byte_end, specification, paths }
    }

    /// Stable requirement identity.
    #[must_use]
    pub const fn id(&self) -> (value: RequirementId)
        ensures value == self.spec_id(),
    { self.id }

    /// Inclusive public-source byte offset.
    #[must_use]
    pub const fn byte_start(&self) -> (value: usize)
        ensures value == self.spec_byte_start(),
    { self.byte_start }

    /// Exclusive public-source byte offset.
    #[must_use]
    pub const fn byte_end(&self) -> (value: usize)
        ensures value == self.spec_byte_end(),
    { self.byte_end }

    /// Proposed typed requirement.
    #[must_use]
    pub const fn specification(&self) -> (value: &ObligationSpec)
        ensures *value == self.spec_specification(),
    { &self.specification }

    /// Exact classified path mentions.
    #[must_use]
    pub const fn paths(&self) -> (value: &[PathMention])
        ensures value@ == self.spec_paths(),
    { self.paths.as_slice() }
}

impl Clone for RequirementDraft {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        Self {
            id: self.id,
            byte_start: self.byte_start,
            byte_end: self.byte_end,
            specification: self.specification.clone(),
            paths: PathMention::clone_sequence(self.paths.as_slice()),
        }
    }
}

/// Canonical exact-clause requirement ledger for one public task source.
#[derive(Debug, Eq, PartialEq)]
pub struct RequirementLedger {
    source_digest: Sha256Digest,
    conversation_revision: u64,
    digest: Sha256Digest,
    entries: Vec<RequirementEntry>,
    limits: ObligationLimits,
}

impl RequirementLedger {
    /// Complete input-defined admission for exact extraction.
    pub open spec fn extraction_inputs_valid(
        source: &crate::PublicTaskSource,
        drafts: Seq<RequirementDraft>,
        limits: ObligationLimits,
    ) -> bool {
        model::extraction_inputs_valid(source, drafts, limits)
    }

    /// Complete first extraction error over the supplied source, drafts, and bounds.
    pub open spec fn extraction_error(
        source: &crate::PublicTaskSource,
        drafts: Seq<RequirementDraft>,
        limits: ObligationLimits,
        error: &ObligationError,
    ) -> bool {
        admission::extraction_error(source, drafts, limits, error)
    }

    /// Exact supplied public-source digest.
    pub closed spec fn spec_source_digest(&self) -> Sha256Digest { self.source_digest }
    /// Exact public conversation revision.
    pub closed spec fn spec_conversation_revision(&self) -> u64 { self.conversation_revision }
    /// Exact ordinary SHA-256 result stored for the canonical extraction bytes.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }
    /// Complete deterministic byte encoding supplied to the ordinary SHA-256 boundary.
    pub open spec fn spec_canonical_bytes(&self) -> Seq<u8> {
        crate::canonical::model::ledger_encoding(self)
    }
    /// Exact retained requirement sequence.
    pub closed spec fn spec_entries(&self) -> Seq<RequirementEntry> { self.entries@ }
    /// Exact retained bounds.
    pub closed spec fn spec_limits(&self) -> ObligationLimits { self.limits }

    /// Canonical stored invariants independent of digest-authenticity assumptions.
    pub open spec fn spec_valid(&self) -> bool {
        model::ledger_valid(
            self.spec_source_digest(),
            self.spec_conversation_revision(),
            self.spec_entries(),
            self.spec_limits(),
        )
    }

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_valid() }

    /// Complete semantic equality, including the stored ordinary digest result.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        &&& self.spec_source_digest() == other.spec_source_digest()
        &&& self.spec_conversation_revision() == other.spec_conversation_revision()
        &&& self.spec_digest() == other.spec_digest()
        &&& RequirementEntry::sequence_same_content(self.spec_entries(), other.spec_entries())
        &&& self.spec_limits() == other.spec_limits()
    }

    /// Exact source, draft, and retained-entry correspondence after successful extraction.
    pub open spec fn spec_refines_extraction(
        &self,
        source: &crate::PublicTaskSource,
        drafts: Seq<RequirementDraft>,
        limits: ObligationLimits,
    ) -> bool {
        model::ledger_refines_extraction(self, source, drafts, limits)
    }

    pub(crate) fn extract_preimage(
        source: &crate::PublicTaskSource,
        drafts: &[RequirementDraft],
        limits: ObligationLimits,
    ) -> (result: Result<Self, ObligationError>)
        ensures
            result.is_ok() == Self::extraction_inputs_valid(source, drafts@, limits),
            match result {
                Ok(ledger) => ledger.spec_refines_extraction(source, drafts@, limits),
                Err(error) => Self::extraction_error(source, drafts@, limits, &error),
            },
    {
        validation::validate_drafts(source, drafts, limits)?;
        extraction::build(source, drafts, limits)
    }

    /// Complete public source digest supplied at the ordinary source boundary.
    #[must_use]
    pub const fn source_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_source_digest(),
    { self.source_digest }

    /// Public conversation revision.
    #[must_use]
    pub const fn conversation_revision(&self) -> (value: u64)
        ensures value == self.spec_conversation_revision(),
    { self.conversation_revision }

    /// Digest returned by the ordinary canonical SHA-256 boundary.
    #[must_use]
    pub const fn digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.digest }

    /// Requirements in canonical identity order.
    #[must_use]
    pub const fn entries(&self) -> (value: &[RequirementEntry])
        ensures value@ == self.spec_entries(),
    { self.entries.as_slice() }

    /// Bounds used for this ledger.
    #[must_use]
    pub const fn limits(&self) -> (value: ObligationLimits)
        ensures value == self.spec_limits(),
    { self.limits }

    /// Finds the unique requirement with the exact stable identity.
    #[must_use]
    pub fn entry(&self, id: RequirementId) -> (result: Option<&RequirementEntry>)
        ensures match result {
            Some(entry) => model::requirement_key(entry.spec_id()) == model::requirement_key(id)
                && exists |index: int| 0 <= index < self.spec_entries().len()
                    && #[trigger] self.spec_entries()[index].spec_same_content(entry),
            None => forall |index: int| 0 <= index < self.spec_entries().len() ==>
                model::requirement_key(#[trigger] self.spec_entries()[index].spec_id())
                    != model::requirement_key(id),
        },
    {
        proof { use_type_invariant(self); }
        let mut low = 0usize;
        let mut high = self.entries.len();
        while low < high
            invariant
                low <= high <= self.spec_entries().len(),
                crate::order::ordered(model::entry_keys(self.spec_entries())),
                forall |key_index: int| 0 <= key_index < self.spec_entries().len() ==>
                    #[trigger] model::entry_keys(self.spec_entries())[key_index].len() == 32,
                forall |prior: int| 0 <= prior < low ==>
                    model::requirement_key(#[trigger] self.spec_entries()[prior].spec_id())
                        != model::requirement_key(id),
                forall |later: int| high <= later < self.spec_entries().len() ==>
                    model::requirement_key(#[trigger] self.spec_entries()[later].spec_id())
                        != model::requirement_key(id),
            decreases high - low,
        {
            let middle = low + (high - low) / 2;
            assert(low <= middle < high);
            let order = crate::order::compare(
                self.entries[middle].id().digest().as_bytes(),
                id.digest().as_bytes(),
            );
            match order {
                Ordering::Equal => {
                    let entry = &self.entries[middle];
                    proof {
                        assert(*entry == self.spec_entries()[middle as int]);
                        assert(model::requirement_key(
                            entry.spec_id())
                            == model::requirement_key(id));
                        entry.entry_same_content_reflexive();
                        assert(self.spec_entries()[middle as int].spec_same_content(
                            entry));
                        assert(exists |index: int| 0 <= index < self.spec_entries().len()
                            && #[trigger] self.spec_entries()[index].spec_same_content(entry)) by {
                            let index = middle as int;
                        }
                        assert(match Some(entry) {
                            Some(returned) => model::requirement_key(returned.spec_id())
                                == model::requirement_key(id)
                                && exists |index: int| 0 <= index < self.spec_entries().len()
                                    && #[trigger] self.spec_entries()[index]
                                        .spec_same_content(returned),
                            None => false,
                        });
                    }
                    return Some(entry);
                },
                Ordering::Less => {
                    proof {
                        crate::order::less_eliminates_through(
                            model::entry_keys(self.spec_entries()),
                            model::requirement_key(id),
                            32,
                            middle as int,
                        );
                        assert forall |prior: int| 0 <= prior <= middle implies
                            model::requirement_key(
                                #[trigger] self.spec_entries()[prior].spec_id())
                                != model::requirement_key(id) by {
                            assert(model::entry_keys(self.spec_entries())[prior]
                                == model::requirement_key(
                                    self.spec_entries()[prior].spec_id()));
                        }
                    }
                    low = middle + 1;
                },
                Ordering::Greater => {
                    proof {
                        crate::order::greater_eliminates_from(
                            model::entry_keys(self.spec_entries()),
                            model::requirement_key(id),
                            32,
                            middle as int,
                        );
                        assert forall |later: int|
                            middle <= later < self.spec_entries().len() implies
                            model::requirement_key(
                                #[trigger] self.spec_entries()[later].spec_id())
                                != model::requirement_key(id) by {
                            assert(model::entry_keys(self.spec_entries())[later]
                                == model::requirement_key(
                                    self.spec_entries()[later].spec_id()));
                        }
                    }
                    high = middle;
                },
            }
        }
        None
    }
}

impl Clone for RequirementLedger {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        proof { use_type_invariant(self); }
        let entries = RequirementEntry::clone_sequence(self.entries.as_slice());
        proof {
            model::clone_preserves_ledger_valid(
                self.spec_source_digest(),
                self.spec_conversation_revision(),
                self.spec_entries(),
                entries@,
                self.spec_limits(),
            );
        }
        Self {
            source_digest: self.source_digest,
            conversation_revision: self.conversation_revision,
            digest: self.digest,
            entries,
            limits: self.limits,
        }
    }
}

} // verus!

#[cfg(not(verus_only))]
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
