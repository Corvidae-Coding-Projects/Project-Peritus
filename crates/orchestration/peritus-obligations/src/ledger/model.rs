//! Input-defined ledger extraction, provenance, ordering, and topology model.

use super::{RequirementDraft, RequirementLedger};
use crate::{
    AlternativeBranchId, AlternativeGroupId, ObligationLimits, PathMention, PublicTaskSource,
    RequirementEntry,
};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Every byte of one requirement identity.
pub open spec fn requirement_key(id: RequirementId) -> Seq<u8> {
    id.spec_digest().spec_bytes()@
}

/// Requirement identities of drafts in their supplied order.
pub open spec fn draft_keys(drafts: Seq<RequirementDraft>) -> Seq<Seq<u8>> {
    drafts.map(|_index: int, draft: RequirementDraft| requirement_key(draft.spec_id()))
}

/// Requirement identities of retained entries in stored order.
pub open spec fn entry_keys(entries: Seq<RequirementEntry>) -> Seq<Seq<u8>> {
    entries.map(|_index: int, entry: RequirementEntry| requirement_key(entry.spec_id()))
}

/// Selects one adjacent comparison from canonical sequence ordering.
pub proof fn ordered_at(keys: Seq<Seq<u8>>, index: int)
    requires crate::order::ordered(keys), 1 <= index < keys.len(),
    ensures crate::order::byte_order(keys[index - 1], keys[index])
        == core::cmp::Ordering::Less,
{
}

/// Exact validity of one proposed source span.
pub open spec fn span_valid(
    source: &PublicTaskSource,
    draft: &RequirementDraft,
    limits: ObligationLimits,
) -> bool {
    draft.spec_byte_start() < draft.spec_byte_end()
        && draft.spec_byte_end() <= source.spec_content().len()
        && draft.spec_byte_end() - draft.spec_byte_start() <= limits.spec_max_clause_bytes()
}

/// Exact intrinsic validity of one draft at its eventual ordinal.
pub open spec fn draft_valid(
    source: &PublicTaskSource,
    draft: &RequirementDraft,
    index: int,
    limits: ObligationLimits,
) -> bool {
    &&& span_valid(source, draft, limits)
    &&& 0 <= index <= u32::MAX as int
    &&& draft.spec_specification().spec_valid()
    &&& draft.spec_paths().len() <= limits.spec_max_paths()
    &&& crate::order::ordered(PathMention::keys(draft.spec_paths()))
}

/// Exact group identity equality.
pub open spec fn same_group(left: AlternativeGroupId, right: AlternativeGroupId) -> bool {
    left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@
}

/// Exact branch identity equality.
pub open spec fn same_branch(left: AlternativeBranchId, right: AlternativeBranchId) -> bool {
    left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@
}

/// Every alternative draft has another member of the same group on a distinct branch.
pub open spec fn draft_alternatives_valid(drafts: Seq<RequirementDraft>) -> bool {
    forall |index: int| 0 <= index < drafts.len() ==>
        match #[trigger] drafts[index].spec_specification().spec_alternative() {
            Some((group, branch)) => exists |other: int| 0 <= other < drafts.len()
                && match #[trigger] drafts[other].spec_specification().spec_alternative() {
                    Some((other_group, other_branch)) => same_group(group, other_group)
                        && !same_branch(branch, other_branch),
                    None => false,
                },
            None => true,
        }
}

/// Every retained alternative has another member of the same group on a distinct branch.
pub open spec fn entry_alternatives_valid(entries: Seq<RequirementEntry>) -> bool {
    forall |index: int| 0 <= index < entries.len() ==>
        match #[trigger] entries[index].spec_specification().spec_alternative() {
            Some((group, branch)) => exists |other: int| 0 <= other < entries.len()
                && match #[trigger] entries[other].spec_specification().spec_alternative() {
                    Some((other_group, other_branch)) => same_group(group, other_group)
                        && !same_branch(branch, other_branch),
                    None => false,
                },
            None => true,
        }
}

/// Complete input-defined ledger-extraction admission.
pub open spec fn extraction_inputs_valid(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    limits: ObligationLimits,
) -> bool {
    &&& 0 < drafts.len() <= limits.spec_max_requirements()
    &&& crate::order::ordered(draft_keys(drafts))
    &&& forall |index: int| 0 <= index < drafts.len() ==>
        draft_valid(source, &drafts[index], index, limits)
    &&& draft_alternatives_valid(drafts)
}

/// One retained entry is the exact source-derived image of one supplied draft.
pub open spec fn entry_matches_draft(
    source: &PublicTaskSource,
    draft: &RequirementDraft,
    index: int,
    entry: &RequirementEntry,
) -> bool {
    let provenance = entry.spec_clause().spec_provenance();
    &&& requirement_key(entry.spec_id()) == requirement_key(draft.spec_id())
    &&& entry.spec_clause().spec_exact() == source.spec_content().subrange(
        draft.spec_byte_start() as int,
        draft.spec_byte_end() as int,
    )
    &&& provenance.spec_source_digest() == source.spec_digest()
    &&& provenance.spec_conversation_revision() == source.spec_conversation_revision()
    &&& provenance.spec_ordinal() as int == index
    &&& provenance.spec_byte_start() == draft.spec_byte_start()
    &&& provenance.spec_byte_end() == draft.spec_byte_end()
    &&& draft.spec_specification().spec_same_content(&entry.spec_specification())
    &&& PathMention::sequence_same_content(draft.spec_paths(), entry.spec_paths())
    &&& entry.spec_valid()
}

/// A constructed entry prefix exactly copies the corresponding draft prefix.
pub open spec fn entries_match_draft_prefix(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    entries: Seq<RequirementEntry>,
) -> bool {
    entries.len() <= drafts.len()
        && forall |index: int| 0 <= index < entries.len() ==>
            entry_matches_draft(source, &drafts[index], index, &entries[index])
}

/// Every retained entry exactly copies its corresponding supplied draft.
pub open spec fn entries_match_drafts(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    entries: Seq<RequirementEntry>,
) -> bool {
    entries.len() == drafts.len()
        && entries_match_draft_prefix(source, drafts, entries)
}

/// Stored ledger invariants independent of the unavailable original source bytes.
pub open spec fn ledger_valid(
    source_digest: Sha256Digest,
    conversation_revision: u64,
    entries: Seq<RequirementEntry>,
    limits: ObligationLimits,
) -> bool {
    &&& 0 < entries.len() <= limits.spec_max_requirements()
    &&& crate::order::ordered(entry_keys(entries))
    &&& crate::order::unique(entry_keys(entries))
    &&& entry_alternatives_valid(entries)
    &&& forall |index: int| 0 <= index < entries.len() ==> {
        let entry = #[trigger] entries[index];
        let provenance = entry.spec_clause().spec_provenance();
        &&& entry.spec_valid()
        &&& entry.spec_paths().len() <= limits.spec_max_paths()
        &&& provenance.spec_source_digest() == source_digest
        &&& provenance.spec_conversation_revision() == conversation_revision
        &&& provenance.spec_ordinal() as int == index
        &&& provenance.spec_byte_start() < provenance.spec_byte_end()
        &&& entry.spec_clause().spec_exact().len()
            == provenance.spec_byte_end() - provenance.spec_byte_start()
        &&& entry.spec_clause().spec_exact().len() <= limits.spec_max_clause_bytes()
    }
}

/// A ledger is the exact successful image of its source, drafts, and limits.
pub open spec fn ledger_refines_extraction(
    ledger: &RequirementLedger,
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    limits: ObligationLimits,
) -> bool {
    &&& extraction_inputs_valid(source, drafts, limits)
    &&& ledger.spec_source_digest() == source.spec_digest()
    &&& ledger.spec_conversation_revision() == source.spec_conversation_revision()
    &&& ledger.spec_limits() == limits
    &&& entries_match_drafts(source, drafts, ledger.spec_entries())
    &&& ledger.spec_valid()
}

/// Appending the next exact entry extends the exact draft correspondence prefix.
pub proof fn entry_prefix_after_push(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    entries: Seq<RequirementEntry>,
    entry: RequirementEntry,
)
    requires
        entries_match_draft_prefix(source, drafts, entries),
        entries.len() < drafts.len(),
        entry_matches_draft(source, &drafts[entries.len() as int], entries.len() as int, &entry),
    ensures entries_match_draft_prefix(source, drafts, entries.push(entry)),
{
    assert forall |index: int| 0 <= index < entries.push(entry).len() implies
        entry_matches_draft(source, &drafts[index], index, &entries.push(entry)[index]) by {
        if index < entries.len() {
            assert(entries.push(entry)[index] == entries[index]);
        } else {
            assert(index == entries.len());
            assert(entries.push(entry)[index] == entry);
        }
    }
}

/// Complete input validity and exact entry correspondence establish all stored invariants.
pub proof fn extraction_establishes_ledger_valid(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    entries: Seq<RequirementEntry>,
    limits: ObligationLimits,
)
    requires
        extraction_inputs_valid(source, drafts, limits),
        entries_match_drafts(source, drafts, entries),
    ensures ledger_valid(
        source.spec_digest(), source.spec_conversation_revision(), entries, limits),
{
    assert(entry_keys(entries) =~= draft_keys(drafts)) by {
        assert forall |index: int| 0 <= index < entries.len() implies
            #[trigger] entry_keys(entries)[index] == #[trigger] draft_keys(drafts)[index] by {
            assert(entry_matches_draft(source, &drafts[index], index, &entries[index]));
        }
    }
    assert(crate::order::ordered(entry_keys(entries)));
    crate::order::ordered_implies_unique(entry_keys(entries), 32);
    assert(entry_alternatives_valid(entries)) by {
        assert forall |index: int| 0 <= index < entries.len() implies
            match #[trigger] entries[index].spec_specification().spec_alternative() {
                Some((group, branch)) => exists |other: int| 0 <= other < entries.len()
                    && match #[trigger] entries[other].spec_specification().spec_alternative() {
                        Some((other_group, other_branch)) => same_group(group, other_group)
                            && !same_branch(branch, other_branch),
                        None => false,
                    },
                None => true,
            } by {
            assert(entry_matches_draft(source, &drafts[index], index, &entries[index]));
            if let Some((group, branch)) = drafts[index].spec_specification().spec_alternative() {
                let other = choose |other: int| 0 <= other < drafts.len()
                    && match #[trigger] drafts[other].spec_specification().spec_alternative() {
                        Some((other_group, other_branch)) => same_group(group, other_group)
                            && !same_branch(branch, other_branch),
                        None => false,
                    };
                assert(entry_matches_draft(source, &drafts[other], other, &entries[other]));
            }
        }
    }
    assert forall |index: int| 0 <= index < entries.len() implies {
        let entry = #[trigger] entries[index];
        let provenance = entry.spec_clause().spec_provenance();
        &&& entry.spec_valid()
        &&& entry.spec_paths().len() <= limits.spec_max_paths()
        &&& provenance.spec_source_digest() == source.spec_digest()
        &&& provenance.spec_conversation_revision() == source.spec_conversation_revision()
        &&& provenance.spec_ordinal() as int == index
        &&& provenance.spec_byte_start() < provenance.spec_byte_end()
        &&& entry.spec_clause().spec_exact().len()
            == provenance.spec_byte_end() - provenance.spec_byte_start()
        &&& entry.spec_clause().spec_exact().len() <= limits.spec_max_clause_bytes()
    } by {
        assert(entry_matches_draft(source, &drafts[index], index, &entries[index]));
        assert(draft_valid(source, &drafts[index], index, limits));
    }
}

/// Complete semantic entry cloning preserves every stored ledger invariant.
pub proof fn clone_preserves_ledger_valid(
    source_digest: Sha256Digest,
    conversation_revision: u64,
    original: Seq<RequirementEntry>,
    cloned: Seq<RequirementEntry>,
    limits: ObligationLimits,
)
    requires
        ledger_valid(source_digest, conversation_revision, original, limits),
        RequirementEntry::sequence_same_content(original, cloned),
    ensures ledger_valid(source_digest, conversation_revision, cloned, limits),
{
    assert(entry_keys(original) =~= entry_keys(cloned)) by {
        assert forall |index: int| 0 <= index < original.len() implies
            #[trigger] entry_keys(original)[index] == #[trigger] entry_keys(cloned)[index] by {
            assert(original[index].spec_same_content(&cloned[index]));
        }
    }
    assert(entry_alternatives_valid(cloned)) by {
        assert forall |index: int| 0 <= index < cloned.len() implies
            match #[trigger] cloned[index].spec_specification().spec_alternative() {
                Some((group, branch)) => exists |other: int| 0 <= other < cloned.len()
                    && match #[trigger] cloned[other].spec_specification().spec_alternative() {
                        Some((other_group, other_branch)) => same_group(group, other_group)
                            && !same_branch(branch, other_branch),
                        None => false,
                    },
                None => true,
            } by {
            assert(original[index].spec_same_content(&cloned[index]));
            if let Some((group, branch)) = original[index].spec_specification().spec_alternative() {
                let other = choose |other: int| 0 <= other < original.len()
                    && match #[trigger] original[other].spec_specification().spec_alternative() {
                        Some((other_group, other_branch)) => same_group(group, other_group)
                            && !same_branch(branch, other_branch),
                        None => false,
                    };
                assert(original[other].spec_same_content(&cloned[other]));
            }
        }
    }
    assert forall |index: int| 0 <= index < cloned.len() implies {
        let entry = #[trigger] cloned[index];
        let provenance = entry.spec_clause().spec_provenance();
        &&& entry.spec_valid()
        &&& entry.spec_paths().len() <= limits.spec_max_paths()
        &&& provenance.spec_source_digest() == source_digest
        &&& provenance.spec_conversation_revision() == conversation_revision
        &&& provenance.spec_ordinal() as int == index
        &&& provenance.spec_byte_start() < provenance.spec_byte_end()
        &&& entry.spec_clause().spec_exact().len()
            == provenance.spec_byte_end() - provenance.spec_byte_start()
        &&& entry.spec_clause().spec_exact().len() <= limits.spec_max_clause_bytes()
    } by {
        assert(original[index].spec_same_content(&cloned[index]));
        original[index].same_content_preserves_valid(&cloned[index]);
        assert(original[index].spec_clause().spec_same_content(
            &cloned[index].spec_clause()));
        assert(original[index].spec_paths().len() == cloned[index].spec_paths().len());
    }
}

} // verus!
