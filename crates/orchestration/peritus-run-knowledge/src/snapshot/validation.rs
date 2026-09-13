//! Exact executable validation of canonical snapshot topology.

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind,
};
use core::cmp::Ordering;
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

pub(super) fn validate_sections(
    candidate: &CandidateIdentity,
    role: HarnessRole,
    sections: &[KnowledgeSection],
) -> (result: Result<(), KnowledgeError>)
    ensures result.is_ok() == crate::model::sections_valid(*candidate, role, sections@),
        result.is_ok() ==> crate::model::section_ids_unique(sections@),
        match result {
            Ok(()) => true,
            Err(error) => crate::model::sections_validation_error(*candidate, role, sections@, error),
        },
{
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            crate::model::sections_canonical_through(sections@, index as nat),
            crate::model::dependencies_precede_through(sections@, index as nat),
            crate::model::section_bindings_valid_through(*candidate, role, sections@, index as nat),
        decreases sections.len() - index,
    {
        let section = &sections[index];
        if let Err(error) = validate_order(sections, index) {
            assert(!crate::model::sections_valid(*candidate, role, sections@));
            proof { record_section_error(*candidate, role, sections@, index as int, error); }
            return Err(error);
        }
        assert(crate::model::sections_canonical_through(
            sections@, index as nat + 1)) by {
            assert forall |ordered_index: int| 1 <= ordered_index < index + 1 implies
                #[trigger] sections@[ordered_index - 1].spec_id().spec_order(
                    &sections@[ordered_index].spec_id()) == Ordering::Less by {
                if ordered_index < index {
                    assert(crate::model::sections_canonical_through(
                        sections@, index as nat));
                } else {
                    assert(ordered_index == index);
                }
            }
        }
        if !section.binding().candidate().same_lineage(candidate) {
            let error = KnowledgeError::section(KnowledgeErrorKind::CandidateLineageMismatch, section.id());
            assert(!crate::model::binding_valid(*candidate, role, *section));
            proof { record_section_error(*candidate, role, sections@, index as int, error); }
            return Err(error);
        }
        if !crate::binding::roles_match(section.binding().role(), role) {
            let error = KnowledgeError::section(KnowledgeErrorKind::RoleMismatch, section.id());
            assert(!crate::model::binding_valid(*candidate, role, *section));
            proof { record_section_error(*candidate, role, sections@, index as int, error); }
            return Err(error);
        }
        if section.binding().creation_sequence() > candidate.checkpoint_sequence() {
            let error = KnowledgeError::section(KnowledgeErrorKind::FutureKnowledge, section.id());
            assert(!crate::model::binding_valid(*candidate, role, *section));
            proof { record_section_error(*candidate, role, sections@, index as int, error); }
            return Err(error);
        }
        assert(crate::model::binding_valid(*candidate, role, *section));
        if let Err(error) = validate_dependencies(sections, index) {
            assert(!crate::model::dependencies_precede(sections@));
            proof { record_section_error(*candidate, role, sections@, index as int, error); }
            return Err(error);
        }
        assert(crate::model::dependencies_precede_through(
            sections@, index as nat + 1)) by {
            assert forall |section_index: int, dependency: int|
                0 <= section_index < index + 1
                    && 0 <= dependency
                        < sections@[section_index].spec_dependencies().len() implies
                    crate::model::dependency_resolves_before(
                        sections@,
                        section_index,
                        sections@[section_index].spec_dependencies()[dependency],
                    ) by {
                if section_index < index {
                    assert(crate::model::dependencies_precede_through(
                        sections@, index as nat));
                } else {
                    assert(section_index == index);
                    assert(sections@[section_index] == *section);
                }
            }
        }
        assert(crate::model::section_bindings_valid_through(*candidate, role, sections@, index as nat + 1));
        index += 1;
    }
    proof {
        crate::model::canonical_implies_unique(sections@);
    }
    Ok(())
}

proof fn record_section_error(
    candidate: CandidateIdentity, role: HarnessRole, sections: Seq<KnowledgeSection>, index: int,
    error: KnowledgeError,
)
    requires crate::model::section_validation_error_at(candidate, role, sections, index, error),
    ensures crate::model::sections_validation_error(candidate, role, sections, error),
{
}

fn validate_order(
    sections: &[KnowledgeSection],
    index: usize,
) -> (result: Result<(), KnowledgeError>)
    requires index < sections.len(),
    ensures result.is_ok() == (index == 0
        || sections@[index as int - 1].spec_id().spec_order(
            &sections@[index as int].spec_id()) == Ordering::Less),
        match result {
            Ok(()) => true,
            Err(error) => crate::model::section_order_error(sections@, index as int, error),
        },
{
    if index == 0 {
        return Ok(());
    }
    let prior_id = sections[index - 1].id();
    let section_id = sections[index].id();
    match prior_id.canonical_order(&section_id) {
        Ordering::Equal => Err(KnowledgeError::section(
            KnowledgeErrorKind::DuplicateValue,
            section_id,
        )),
        Ordering::Greater => Err(KnowledgeError::section(
            KnowledgeErrorKind::NonCanonicalOrder,
            section_id,
        )),
        Ordering::Less => Ok(()),
    }
}

fn validate_dependencies(
    sections: &[KnowledgeSection],
    index: usize,
) -> (result: Result<(), KnowledgeError>)
    requires index < sections.len(),
    ensures result.is_ok() == (forall |dependency: int|
        0 <= dependency < sections@[index as int].spec_dependencies().len() ==>
            crate::model::dependency_resolves_before(
                sections@,
                index as int,
                #[trigger] sections@[index as int].spec_dependencies()[dependency],
            )),
        match result {
            Ok(()) => true,
            Err(error) => crate::model::section_dependency_error(sections@, index as int, error),
        },
{
    let section = &sections[index];
    let dependencies = section.dependencies();
    let mut dependency_index = 0;
    while dependency_index < dependencies.len()
        invariant
            dependency_index <= dependencies.len(),
            index < sections.len(),
            dependencies@ == section.spec_dependencies(),
            *section == sections@[index as int],
            forall |prior_dependency: int|
                0 <= prior_dependency < dependency_index ==>
                    crate::model::dependency_resolves_before(
                        sections@,
                        index as int,
                        dependencies@[prior_dependency],
                    ),
        decreases dependencies.len() - dependency_index,
    {
        let dependency = dependencies[dependency_index];
        match find_section_index(sections, dependency) {
            Some(found) if found < index => {
                assert(crate::model::dependency_resolves_before(
                    sections@, index as int, dependency)) by {
                    assert(sections@[found as int].spec_id().spec_matches(&dependency));
                };
            }
            _ => {
                assert(!crate::model::dependency_resolves_before(sections@, index as int, dependency));
                let error = KnowledgeError::section(KnowledgeErrorKind::InvalidDependency, dependency);
                assert(crate::model::section_dependency_error(sections@, index as int, error)) by {
                    assert(crate::model::section_dependency_error_at(
                        sections@, index as int, dependency_index as int, error));
                }
                return Err(error);
            }
        }
        dependency_index += 1;
    }
    Ok(())
}

fn find_section_index(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
) -> (result: Option<usize>)
    ensures crate::model::section_index_result(sections@, id, result),
{
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            forall |prior: int| 0 <= prior < index ==>
                !sections@[prior].spec_id().spec_matches(&id),
        decreases sections.len() - index,
    {
        if sections[index].id().matches(&id) {
            return Some(index);
        }
        assert(!sections@[index as int].spec_id().spec_matches(&id));
        index += 1;
    }
    None
}

pub(super) fn reference_has_kind(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
) -> (result: bool)
    ensures crate::model::reference_has_kind_result(sections@, id, kind, result),
{
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            forall |prior: int| 0 <= prior < index ==>
                !(sections@[prior].spec_id().spec_matches(&id)
                    && sections@[prior].spec_kind() == kind),
        decreases sections.len() - index,
    {
        let section = &sections[index];
        let id_matches = section.id().matches(&id);
        let section_kind = section.kind();
        assert(*section == sections@[index as int]);
        let kind_matches = section_kind.matches(kind);
        if id_matches && kind_matches {
            assert(crate::model::required_section_has_kind(sections@, id, kind)) by {
                let witness = index as int;
                assert(sections@[witness].spec_id().spec_matches(&id));
                assert(sections@[witness].spec_kind() == kind);
            }
            return true;
        }
        assert(!(sections@[index as int].spec_id().spec_matches(&id)
            && sections@[index as int].spec_kind() == kind));
        index += 1;
    }
    false
}

} // verus!
