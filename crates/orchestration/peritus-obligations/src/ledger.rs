//! Deterministic extraction of typed requirements from exact public source spans.

use crate::{
    ObligationError, ObligationErrorKind, ObligationLimits, ObligationSpec, PathMention,
    PublicClause, PublicTaskSource, RequirementEntry,
};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use std::collections::{BTreeMap, BTreeSet};

/// One proposed typed extraction retaining its exact public byte span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementDraft {
    id: RequirementId,
    byte_start: usize,
    byte_end: usize,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
}

impl RequirementDraft {
    /// Creates an inert extraction proposal. [`RequirementLedger::extract`] validates the span,
    /// ordering, typed details, paths, and alternative topology against the public source.
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

    /// Stable requirement identity.
    #[must_use]
    pub const fn id(&self) -> RequirementId {
        self.id
    }

    /// Inclusive public-source byte offset.
    #[must_use]
    pub const fn byte_start(&self) -> usize {
        self.byte_start
    }

    /// Exclusive public-source byte offset.
    #[must_use]
    pub const fn byte_end(&self) -> usize {
        self.byte_end
    }

    /// Proposed typed requirement.
    #[must_use]
    pub const fn specification(&self) -> &ObligationSpec {
        &self.specification
    }

    /// Exact classified path mentions.
    #[must_use]
    pub const fn paths(&self) -> &[PathMention] {
        self.paths.as_slice()
    }
}

/// Canonical exact-clause requirement ledger for one public task source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementLedger {
    source_digest: Sha256Digest,
    conversation_revision: u64,
    digest: Sha256Digest,
    entries: Vec<RequirementEntry>,
    limits: ObligationLimits,
}

impl RequirementLedger {
    /// Validates typed extraction proposals and copies the exact public clauses into a ledger.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized collections, unordered or duplicate identities, invalid source
    /// spans, incompatible typed details, noncanonical paths, and alternative groups with fewer
    /// than two distinct branches.
    pub fn extract(
        source: &PublicTaskSource,
        drafts: Vec<RequirementDraft>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        if drafts.is_empty() || drafts.len() > limits.max_requirements() {
            return Err(ObligationError::numbers(
                ObligationErrorKind::LimitExceeded,
                limits.max_requirements() as u64,
                drafts.len() as u64,
            ));
        }
        let mut entries: Vec<RequirementEntry> = Vec::with_capacity(drafts.len());
        for (index, draft) in drafts.into_iter().enumerate() {
            if let Some(previous) = entries.last() {
                if previous.id() == draft.id {
                    return Err(ObligationError::requirement(
                        ObligationErrorKind::DuplicateValue,
                        draft.id,
                    ));
                }
                if previous.id() > draft.id {
                    return Err(ObligationError::requirement(
                        ObligationErrorKind::NonCanonicalOrder,
                        draft.id,
                    ));
                }
            }
            let clause_length = draft.byte_end.saturating_sub(draft.byte_start);
            if draft.byte_start >= draft.byte_end
                || draft.byte_end > source.content().len()
                || clause_length > limits.max_clause_bytes()
            {
                return Err(ObligationError::requirement(
                    ObligationErrorKind::InvalidClauseSpan,
                    draft.id,
                ));
            }
            let ordinal = u32::try_from(index).map_err(|_| {
                ObligationError::requirement(ObligationErrorKind::LimitExceeded, draft.id)
            })?;
            let provenance = crate::ClauseProvenance::new(
                source.digest(),
                source.conversation_revision(),
                ordinal,
                draft.byte_start,
                draft.byte_end,
            );
            let clause = PublicClause::new(
                source.content()[draft.byte_start..draft.byte_end].to_vec(),
                provenance,
            );
            entries.push(RequirementEntry::new(
                draft.id,
                clause,
                draft.specification,
                draft.paths,
                limits,
            )?);
        }
        validate_alternatives(entries.as_slice())?;
        let mut ledger = Self {
            source_digest: source.digest(),
            conversation_revision: source.conversation_revision(),
            digest: Sha256Digest::new([0; 32]),
            entries,
            limits,
        };
        ledger.digest =
            crate::canonical::sha256(crate::canonical::ledger_bytes(&ledger).as_slice());
        Ok(ledger)
    }

    /// Complete public source digest.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }

    /// Public conversation revision.
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }

    /// Digest of the exact extraction, including clauses and typed details.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Requirements in canonical identity order.
    #[must_use]
    pub const fn entries(&self) -> &[RequirementEntry] {
        self.entries.as_slice()
    }

    /// Bounds used for this ledger.
    #[must_use]
    pub const fn limits(&self) -> ObligationLimits {
        self.limits
    }

    /// Finds a requirement by stable identity.
    #[must_use]
    pub fn entry(&self, id: RequirementId) -> Option<&RequirementEntry> {
        self.entries
            .binary_search_by_key(&id, RequirementEntry::id)
            .ok()
            .map(|index| &self.entries[index])
    }
}

fn validate_alternatives(entries: &[RequirementEntry]) -> Result<(), ObligationError> {
    let mut groups = BTreeMap::new();
    for entry in entries {
        if let Some((group, branch)) = entry.specification().alternative() {
            groups.entry(group).or_insert_with(BTreeSet::new).insert(branch);
        }
    }
    if groups.values().any(|branches| branches.len() < 2) {
        Err(ObligationError::plain(ObligationErrorKind::InvalidAlternative))
    } else {
        Ok(())
    }
}
