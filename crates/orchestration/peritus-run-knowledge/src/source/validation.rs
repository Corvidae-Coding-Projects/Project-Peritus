//! Checked source admission with exact size, order, and first-error contracts.

use crate::{KnowledgeError, KnowledgeErrorKind, SourceDigest};
use core::cmp::Ordering;

pub fn validate_sources(
    sources: &[SourceDigest],
    maximum: usize,
    allow_empty: bool,
) -> Result<(), KnowledgeError> {
    if !allow_empty && sources.is_empty() {
        return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
    }
    if sources.len() > maximum {
        return Err(KnowledgeError::numbers(
            KnowledgeErrorKind::LimitExceeded,
            maximum as u64,
            sources.len() as u64,
        ));
    }
    let mut index = 0;
    while index < sources.len() {
        if index > 0 {
            let previous = sources[index - 1].source_id();
            let current = sources[index].source_id();
            match previous.canonical_order(&current) {
                Ordering::Equal => {
                    let error = KnowledgeError::source(KnowledgeErrorKind::DuplicateValue, current);
                    return Err(error);
                }
                Ordering::Greater => {
                    let error =
                        KnowledgeError::source(KnowledgeErrorKind::NonCanonicalOrder, current);
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
