//! Exact source-span copying after verified extraction admission.

use super::{RequirementDraft, RequirementLedger};
use crate::{
    ClauseProvenance, ObligationError, ObligationErrorKind, ObligationLimits, PathMention,
    PublicClause, PublicTaskSource,
};
use peritus_types::Sha256Digest;

pub(super) fn build(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    limits: ObligationLimits,
) -> Result<RequirementLedger, ObligationError> {
    let mut entries = Vec::with_capacity(drafts.len());
    let mut index = 0;
    while index < drafts.len() {
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
        let entry = crate::RequirementEntry::new(draft.id(), clause, specification, paths, limits)?;
        entries.push(entry);
        index += 1;
    }
    Ok(RequirementLedger {
        source_digest: source.digest(),
        conversation_revision: source.conversation_revision(),
        digest: Sha256Digest::new([0; 32]),
        entries,
        limits,
    })
}
fn copy_span(source: &[u8], start: usize, end: usize) -> Vec<u8> {
    let mut exact = Vec::with_capacity(end - start);
    let mut index = start;
    while index < end {
        exact.push(source[index]);
        index += 1;
    }
    exact
}
