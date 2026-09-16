//! Input-defined snapshot lineage, role, time, topology, and exact admission errors.

#[cfg(verus_only)]
use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeSection};
#[cfg(verus_only)]
use core::cmp::Ordering;
#[cfg(verus_only)]
use peritus_role::HarnessRole;
#[cfg(verus_only)]
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Existing snapshot policy permits older candidate content but requires the same run and workspace.
pub open spec fn binding_valid(
    candidate: CandidateIdentity, role: HarnessRole, section: KnowledgeSection,
) -> bool {
    section.spec_binding().spec_candidate().spec_same_lineage(&candidate)
        && section.spec_binding().spec_role() == role
        && section.spec_binding().spec_creation_sequence() <= candidate.spec_checkpoint_sequence()
}

/// All already examined bindings satisfy the supplied snapshot lineage, role, and time.
pub open spec fn section_bindings_valid_through(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>, end: nat,
) -> bool {
    end <= sections.len() && forall |index: int| 0 <= index < end ==>
        #[trigger] binding_valid(candidate, role, sections[index])
}

/// Exact admission of the examined section prefix, before required references are checked.
pub open spec fn sections_valid_through(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>, end: nat,
) -> bool {
    crate::model::sections_canonical_through(sections, end)
        && crate::model::dependencies_precede_through(sections, end)
        && section_bindings_valid_through(candidate, role, sections, end)
}

/// Exact section validation independent of allocation bounds and required reference checks.
pub open spec fn sections_valid(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>,
) -> bool { sections_valid_through(candidate, role, sections, sections.len()) }

/// Error category and full identity from an invalid adjacent section pair.
pub open spec fn section_order_error(
    sections: Seq<KnowledgeSection>, index: int, error: KnowledgeError,
) -> bool {
    1 <= index < sections.len()
        && match sections[index - 1].spec_id().spec_order(&sections[index].spec_id()) {
            Ordering::Equal => error.spec_section(KnowledgeErrorKind::DuplicateValue, sections[index].spec_id()),
            Ordering::Greater => error.spec_section(KnowledgeErrorKind::NonCanonicalOrder, sections[index].spec_id()),
            Ordering::Less => false,
        }
}

/// Exact per-binding failure precedence: lineage, role, then creation after checkpoint.
pub open spec fn section_binding_error(
    candidate: CandidateIdentity, role: HarnessRole, section: KnowledgeSection, error: KnowledgeError,
) -> bool {
    if !section.spec_binding().spec_candidate().spec_same_lineage(&candidate) {
        error.spec_section(KnowledgeErrorKind::CandidateLineageMismatch, section.spec_id())
    } else if section.spec_binding().spec_role() != role {
        error.spec_section(KnowledgeErrorKind::RoleMismatch, section.spec_id())
    } else {
        section.spec_binding().spec_creation_sequence() > candidate.spec_checkpoint_sequence()
            && error.spec_section(KnowledgeErrorKind::FutureKnowledge, section.spec_id())
    }
}

/// First dependency that does not resolve to an earlier snapshot section.
pub open spec fn section_dependency_error_at(
    sections: Seq<KnowledgeSection>, index: int, dependency_index: int, error: KnowledgeError,
) -> bool {
    0 <= index < sections.len()
        && 0 <= dependency_index < sections[index].spec_dependencies().len()
        && (forall |prior: int| 0 <= prior < dependency_index ==>
            crate::model::dependency_resolves_before(sections, index,
                #[trigger] sections[index].spec_dependencies()[prior]))
        && !crate::model::dependency_resolves_before(sections, index,
            sections[index].spec_dependencies()[dependency_index])
        && error.spec_section(KnowledgeErrorKind::InvalidDependency,
            sections[index].spec_dependencies()[dependency_index])
}

/// Complete first dependency error for one supplied section.
pub open spec fn section_dependency_error(
    sections: Seq<KnowledgeSection>, index: int, error: KnowledgeError,
) -> bool {
    exists |dependency: int| #[trigger] section_dependency_error_at(sections, index, dependency, error)
}

/// First section failure, preserving order before binding before dependency checks.
pub open spec fn section_validation_error_at(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>, index: int,
    error: KnowledgeError,
) -> bool {
    0 <= index < sections.len() && sections_valid_through(candidate, role, sections, index as nat)
        && if index > 0 && sections[index - 1].spec_id().spec_order(&sections[index].spec_id()) != Ordering::Less {
            section_order_error(sections, index, error)
        } else if !binding_valid(candidate, role, sections[index]) {
            section_binding_error(candidate, role, sections[index], error)
        } else { section_dependency_error(sections, index, error) }
}

/// Complete exact error for validating the supplied section sequence.
pub open spec fn sections_validation_error(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>, error: KnowledgeError,
) -> bool {
    exists |index: int| #[trigger] section_validation_error_at(candidate, role, sections, index, error)
}

} // verus!
