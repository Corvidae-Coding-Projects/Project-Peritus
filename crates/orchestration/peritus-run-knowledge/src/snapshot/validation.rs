//! Exact executable validation of canonical snapshot topology.

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind,
};
use core::cmp::Ordering;
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;

pub(super) fn validate_sections(
    candidate: &CandidateIdentity,
    role: HarnessRole,
    sections: &[KnowledgeSection],
) -> Result<(), KnowledgeError> {
    let mut index = 0;
    while index < sections.len() {
        let section = &sections[index];
        validate_order(sections, index)?;
        if !section.binding().candidate().same_lineage(candidate) {
            let error =
                KnowledgeError::section(KnowledgeErrorKind::CandidateLineageMismatch, section.id());
            return Err(error);
        }
        if !crate::binding::roles_match(section.binding().role(), role) {
            let error = KnowledgeError::section(KnowledgeErrorKind::RoleMismatch, section.id());
            return Err(error);
        }
        if section.binding().creation_sequence() > candidate.checkpoint_sequence() {
            let error = KnowledgeError::section(KnowledgeErrorKind::FutureKnowledge, section.id());
            return Err(error);
        }
        validate_dependencies(sections, index)?;
        index += 1;
    }
    Ok(())
}
fn validate_order(sections: &[KnowledgeSection], index: usize) -> Result<(), KnowledgeError> {
    if index == 0 {
        return Ok(());
    }
    let prior_id = sections[index - 1].id();
    let section_id = sections[index].id();
    match prior_id.canonical_order(&section_id) {
        Ordering::Equal => {
            Err(KnowledgeError::section(KnowledgeErrorKind::DuplicateValue, section_id))
        }
        Ordering::Greater => {
            Err(KnowledgeError::section(KnowledgeErrorKind::NonCanonicalOrder, section_id))
        }
        Ordering::Less => Ok(()),
    }
}
fn validate_dependencies(
    sections: &[KnowledgeSection],
    index: usize,
) -> Result<(), KnowledgeError> {
    let section = &sections[index];
    let dependencies = section.dependencies();
    let mut dependency_index = 0;
    while dependency_index < dependencies.len() {
        let dependency = dependencies[dependency_index];
        match find_section_index(sections, dependency) {
            Some(found) if found < index => {}
            _ => {
                let error =
                    KnowledgeError::section(KnowledgeErrorKind::InvalidDependency, dependency);
                return Err(error);
            }
        }
        dependency_index += 1;
    }
    Ok(())
}
fn find_section_index(sections: &[KnowledgeSection], id: KnowledgeSectionId) -> Option<usize> {
    let mut index = 0;
    while index < sections.len() {
        if sections[index].id().matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(super) fn reference_has_kind(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
) -> bool {
    let mut index = 0;
    while index < sections.len() {
        let section = &sections[index];
        let id_matches = section.id().matches(&id);
        let section_kind = section.kind();
        let kind_matches = section_kind.matches(kind);
        if id_matches && kind_matches {
            return true;
        }
        index += 1;
    }
    false
}
