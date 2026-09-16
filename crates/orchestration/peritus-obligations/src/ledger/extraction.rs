//! Exact source-span copying after verified extraction admission.

#[cfg(verus_only)]
use super::model;
use super::{RequirementDraft, RequirementLedger};
use crate::{
    ClauseProvenance, ObligationError, ObligationErrorKind, ObligationLimits, PathMention,
    PublicClause, PublicTaskSource,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub(super) fn build(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    limits: ObligationLimits,
) -> (result: Result<RequirementLedger, ObligationError>)
    requires model::extraction_inputs_valid(source, drafts@, limits),
    ensures
        result.is_ok(),
        match result {
            Ok(ledger) => model::ledger_refines_extraction(&ledger, source, drafts@, limits),
            Err(_) => false,
        },
{
    let mut entries = Vec::with_capacity(drafts.len());
    let mut index = 0;
    while index < drafts.len()
        invariant
            index <= drafts@.len(),
            model::extraction_inputs_valid(source, drafts@, limits),
            entries@.len() == index,
            model::entries_match_draft_prefix(source, drafts@, entries@),
        decreases drafts.len() - index,
    {
        let draft = &drafts[index];
        let Ok(ordinal) = u32::try_from(index) else {
            return Err(ObligationError::requirement(
                ObligationErrorKind::LimitExceeded,
                draft.id(),
            ));
        };
        let provenance = ClauseProvenance::new(
            source.digest(),
            source.conversation_revision(),
            ordinal,
            draft.byte_start(),
            draft.byte_end(),
        );
        let exact = copy_span(source.content(), draft.byte_start(), draft.byte_end());
        let clause = PublicClause::new(exact, provenance);
        let specification = draft.specification().clone();
        let paths = PathMention::clone_sequence(draft.paths());
        let entry = crate::RequirementEntry::new(
            draft.id(),
            clause,
            specification,
            paths,
            limits,
        )?;
        assert(model::entry_matches_draft(
            source, draft, index as int, &entry));
        let ghost prior = entries@;
        entries.push(entry);
        proof {
            model::entry_prefix_after_push(source, drafts@, prior, entry);
        }
        index += 1;
    }
    assert(model::entries_match_drafts(source, drafts@, entries@));
    proof {
        model::extraction_establishes_ledger_valid(source, drafts@, entries@, limits);
    }
    Ok(RequirementLedger {
        source_digest: source.digest(),
        conversation_revision: source.conversation_revision(),
        digest: Sha256Digest::new([0; 32]),
        entries,
        limits,
    })
}

fn copy_span(source: &[u8], start: usize, end: usize) -> (exact: Vec<u8>)
    requires start < end <= source@.len(),
    ensures exact@ == source@.subrange(start as int, end as int),
{
    let mut exact = Vec::with_capacity(end - start);
    let mut index = start;
    while index < end
        invariant
            start <= index <= end <= source@.len(),
            exact@ == source@.subrange(start as int, index as int),
        decreases end - index,
    {
        exact.push(source[index]);
        index += 1;
    }
    exact
}

} // verus!
