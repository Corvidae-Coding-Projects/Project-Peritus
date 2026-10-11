//! Production validation of canonical section identity collections.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeSectionId};
use core::cmp::Ordering;

pub fn validate_section_ids(
    ids: &[KnowledgeSectionId],
    excluded: Option<KnowledgeSectionId>,
) -> Result<(), KnowledgeError> {
    let mut index = 0;
    while index < ids.len() {
        match excluded {
            Some(id) if ids[index].matches(&id) => {
                let error = KnowledgeError::section(KnowledgeErrorKind::SelfDependency, id);
                return Err(error);
            }
            _ => {}
        }
        if index > 0 {
            match ids[index - 1].canonical_order(&ids[index]) {
                Ordering::Equal => {
                    let error =
                        KnowledgeError::section(KnowledgeErrorKind::DuplicateValue, ids[index]);
                    return Err(error);
                }
                Ordering::Greater => {
                    let error =
                        KnowledgeError::section(KnowledgeErrorKind::NonCanonicalOrder, ids[index]);
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
