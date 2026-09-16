//! Canonical snapshot topology and dependency-closure predicates.

#[cfg(verus_only)]
use core::cmp::Ordering;
#[cfg(verus_only)]
use crate::{KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind, PlannedKnowledge};
use vstd::prelude::*;

verus! {

/// Complete section identity bytes in caller-supplied order.
pub open spec fn section_keys(sections: Seq<KnowledgeSection>) -> Seq<Seq<u8>> {
    sections.map(|_index: int, section: KnowledgeSection| section.spec_id().spec_bytes()@)
}

/// Every adjacent section identity is in strict canonical byte order.
pub open spec fn sections_canonical(sections: Seq<KnowledgeSection>) -> bool {
    forall |index: int| 1 <= index < sections.len() ==>
        #[trigger] sections[index - 1].spec_id().spec_order(&sections[index].spec_id())
            == Ordering::Less
}

/// A prefix of the supplied sections is in strict canonical byte order.
pub open spec fn sections_canonical_through(
    sections: Seq<KnowledgeSection>,
    end: nat,
) -> bool {
    end <= sections.len()
        && forall |index: int| 1 <= index < end ==>
            #[trigger] sections[index - 1].spec_id().spec_order(&sections[index].spec_id())
                == Ordering::Less
}

/// No two section positions carry the same complete identity bytes.
pub open spec fn section_ids_unique(sections: Seq<KnowledgeSection>) -> bool {
    forall |left: int, right: int| 0 <= left < right < sections.len() ==>
        !#[trigger] sections[left].spec_id().spec_matches(&sections[right].spec_id())
}

/// One dependency identity resolves to a section that appears earlier in the snapshot.
pub open spec fn dependency_resolves_before(
    sections: Seq<KnowledgeSection>,
    section_index: int,
    dependency: KnowledgeSectionId,
) -> bool {
    exists |prior: int| 0 <= prior < section_index
        && prior < sections.len()
        && sections[prior].spec_id().spec_matches(&dependency)
}

/// Every declared dependency in a prefix resolves to an earlier section.
pub open spec fn dependencies_precede_through(
    sections: Seq<KnowledgeSection>,
    end: nat,
) -> bool {
    end <= sections.len()
        && forall |section: int, dependency: int|
            0 <= section < end
                && 0 <= dependency < sections[section].spec_dependencies().len() ==>
                dependency_resolves_before(
                    sections,
                    section,
                    #[trigger] sections[section].spec_dependencies()[dependency],
                )
}

/// Every declared dependency resolves to an earlier section in the canonical sequence.
pub open spec fn dependencies_precede(sections: Seq<KnowledgeSection>) -> bool {
    dependencies_precede_through(sections, sections.len())
}

/// One required section reference resolves to the exact declared kind.
pub open spec fn required_section_has_kind(
    sections: Seq<KnowledgeSection>,
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
) -> bool {
    exists |index: int| 0 <= index < sections.len()
        && sections[index].spec_id().spec_matches(&id)
        && sections[index].spec_kind() == kind
}

/// Exact result of the constructor's internal identity lookup.
pub open spec fn section_index_result(
    sections: Seq<KnowledgeSection>,
    id: KnowledgeSectionId,
    result: Option<usize>,
) -> bool {
    match result {
        Some(index) => index < sections.len()
            && sections[index as int].spec_id().spec_matches(&id)
            && (forall |prior: int| 0 <= prior < index ==>
                !#[trigger] sections[prior].spec_id().spec_matches(&id)),
        None => forall |index: int| 0 <= index < sections.len() ==>
            !#[trigger] sections[index].spec_id().spec_matches(&id),
    }
}

/// Exact result of checking a required section identity and kind.
pub open spec fn reference_has_kind_result(
    sections: Seq<KnowledgeSection>,
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
    result: bool,
) -> bool {
    result == required_section_has_kind(sections, id, kind)
}

/// Canonical identity and dependency facts retained by every admitted snapshot.
pub open spec fn snapshot_topology(
    sections: Seq<KnowledgeSection>,
    repository_inventory: KnowledgeSectionId,
    relevant_file_map: KnowledgeSectionId,
    requirement_ledger: KnowledgeSectionId,
) -> bool {
    &&& sections_canonical(sections)
    &&& section_ids_unique(sections)
    &&& dependencies_precede(sections)
    &&& required_section_has_kind(
        sections, repository_inventory, KnowledgeSectionKind::RepositoryInventory)
    &&& required_section_has_kind(
        sections, relevant_file_map, KnowledgeSectionKind::RelevantFileMap)
    &&& required_section_has_kind(
        sections, requirement_ledger, KnowledgeSectionKind::LiteralRequirementLedger)
}

/// Whether one indexed section directly depends on an earlier indexed section.
pub open spec fn direct_dependency_at(
    sections: Seq<KnowledgeSection>,
    dependent: nat,
    dependency: nat,
) -> bool {
    dependent < sections.len()
        && dependency < dependent
        && exists |position: int|
            0 <= position < sections[dependent as int].spec_dependencies().len()
                && sections[dependent as int].spec_dependencies()[position].spec_matches(
                    &sections[dependency as int].spec_id())
}

/// Exact dependency identity at one section and dependency position.
pub open spec fn dependency_at(
    sections: Seq<KnowledgeSection>,
    dependent: int,
    position: int,
) -> KnowledgeSectionId {
    sections[dependent].spec_dependencies()[position]
}

/// Backward transitive dependency reachability through admitted section positions.
pub open spec fn transitively_depends_on(
    sections: Seq<KnowledgeSection>,
    dependent: nat,
    ancestor: nat,
) -> bool
    decreases dependent,
{
    dependent < sections.len()
        && ancestor < dependent
        && (direct_dependency_at(sections, dependent, ancestor)
            || exists |middle: nat| ancestor < middle < dependent
                && direct_dependency_at(sections, dependent, middle)
                && transitively_depends_on(sections, middle, ancestor))
}

/// Invalid decisions are upward closed over every backward transitive dependency.
pub open spec fn transitive_invalidation_closed(
    sections: Seq<KnowledgeSection>,
    entries: Seq<PlannedKnowledge>,
) -> bool {
    entries.len() == sections.len()
        && forall |dependent: nat, ancestor: nat|
            #[trigger] transitively_depends_on(sections, dependent, ancestor)
                && !entries[ancestor as int].spec_decision().spec_is_reuse() ==>
                    !entries[dependent as int].spec_decision().spec_is_reuse()
}

proof fn order_reflexive_from(bytes: Seq<u8>, index: nat)
    requires index <= bytes.len(),
    ensures peritus_types::canonical_byte_order_from(bytes, bytes, index) == Ordering::Equal,
    decreases bytes.len() - index,
{
    if index < bytes.len() {
        order_reflexive_from(bytes, index + 1);
    }
}

proof fn order_transitive_from(
    left: Seq<u8>,
    middle: Seq<u8>,
    right: Seq<u8>,
    index: nat,
)
    requires
        left.len() == middle.len(),
        middle.len() == right.len(),
        index <= left.len(),
        peritus_types::canonical_byte_order_from(left, middle, index) == Ordering::Less,
        peritus_types::canonical_byte_order_from(middle, right, index) == Ordering::Less,
    ensures peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len()
        && left[index as int] == middle[index as int]
        && middle[index as int] == right[index as int]
    {
        order_transitive_from(left, middle, right, index + 1);
    }
}

proof fn canonical_pair(
    sections: Seq<KnowledgeSection>,
    left: int,
    right: int,
)
    requires sections_canonical(sections), 0 <= left < right < sections.len(),
    ensures sections[left].spec_id().spec_order(&sections[right].spec_id()) == Ordering::Less,
    decreases right - left,
{
    assert(sections[right - 1].spec_id().spec_order(
        &sections[right].spec_id()) == Ordering::Less);
    if left + 1 < right {
        canonical_pair(sections, left, right - 1);
        order_transitive_from(
            sections[left].spec_id().spec_bytes()@,
            sections[right - 1].spec_id().spec_bytes()@,
            sections[right].spec_id().spec_bytes()@,
            0,
        );
    } else {
        assert(left == right - 1);
    }
}

/// Strict canonical ordering proves global section identity uniqueness.
pub proof fn canonical_implies_unique(sections: Seq<KnowledgeSection>)
    requires sections_canonical(sections),
    ensures section_ids_unique(sections),
{
    assert forall |left: int, right: int| 0 <= left < right < sections.len() implies
        !sections[left].spec_id().spec_matches(&sections[right].spec_id()) by {
        canonical_pair(sections, left, right);
        if sections[left].spec_id().spec_matches(&sections[right].spec_id()) {
            assert(sections[left].spec_id().spec_bytes()@
                == sections[right].spec_id().spec_bytes()@);
            order_reflexive_from(sections[left].spec_id().spec_bytes()@, 0);
        }
    }
}

/// One declared dependency has its exact earlier indexed graph edge.
pub proof fn dependency_has_backward_edge(
    sections: Seq<KnowledgeSection>,
    dependent: int,
    position: int,
)
    requires
        dependencies_precede(sections),
        0 <= dependent < sections.len(),
        0 <= position < sections[dependent].spec_dependencies().len(),
    ensures exists |dependency: nat| dependency < dependent
        && direct_dependency_at(sections, dependent as nat, dependency)
        && sections[dependency as int].spec_id().spec_matches(
            &dependency_at(sections, dependent, position)),
{
    let dependency_id = dependency_at(sections, dependent, position);
    assert(dependency_resolves_before(sections, dependent, dependency_id));
    let dependency = choose |prior: int| 0 <= prior < dependent
        && prior < sections.len()
        && sections[prior].spec_id().spec_matches(&dependency_id);
    assert(direct_dependency_at(
        sections, dependent as nat, dependency as nat)) by {
        let dependency_position = position;
        assert(sections[dependent].spec_dependencies()[dependency_position].spec_matches(
            &sections[dependency].spec_id())) by {
            assert(dependency_id.spec_bytes()
                == sections[dependency].spec_id().spec_bytes());
        }
    }
    assert(sections[dependency].spec_id().spec_matches(
        &dependency_at(sections, dependent, position))) by {
        assert(dependency_at(sections, dependent, position) == dependency_id);
    }
    assert(exists |dependency_index: nat| dependency_index < dependent
        && direct_dependency_at(sections, dependent as nat, dependency_index)
        && sections[dependency_index as int].spec_id().spec_matches(
            &dependency_at(sections, dependent, position))) by {
        let dependency_index = dependency as nat;
    }
}

/// Section cloning preserves every canonical identity, kind, and dependency-topology fact.
pub proof fn clone_equivalent_preserves_snapshot_topology(
    left: Seq<KnowledgeSection>,
    right: Seq<KnowledgeSection>,
    repository_inventory: KnowledgeSectionId,
    relevant_file_map: KnowledgeSectionId,
    requirement_ledger: KnowledgeSectionId,
)
    requires
        snapshot_topology(
            left, repository_inventory, relevant_file_map, requirement_ledger),
        KnowledgeSection::sequence_clone_equivalent(left, right),
    ensures snapshot_topology(
        right, repository_inventory, relevant_file_map, requirement_ledger),
{
    assert(sections_canonical(right)) by {
        assert forall |index: int| 1 <= index < right.len() implies
            #[trigger] right[index - 1].spec_id().spec_order(&right[index].spec_id())
                == Ordering::Less by {
            assert(KnowledgeSection::clone_equivalent(&left[index - 1], &right[index - 1]));
            assert(KnowledgeSection::clone_equivalent(&left[index], &right[index]));
            assert(left[index - 1].spec_id() == right[index - 1].spec_id());
            assert(left[index].spec_id() == right[index].spec_id());
        }
    }
    canonical_implies_unique(right);
    assert(dependencies_precede(right)) by {
        assert forall |section: int, dependency: int|
            0 <= section < right.len()
                && 0 <= dependency < right[section].spec_dependencies().len() implies
                dependency_resolves_before(
                    right,
                    section,
                    #[trigger] right[section].spec_dependencies()[dependency],
                ) by {
            assert(KnowledgeSection::clone_equivalent(&left[section], &right[section]));
            assert(left[section].spec_dependencies() == right[section].spec_dependencies());
            let dependency_id = right[section].spec_dependencies()[dependency];
            assert(dependency_resolves_before(left, section, dependency_id));
            let prior = choose |prior: int| 0 <= prior < section
                && prior < left.len()
                && left[prior].spec_id().spec_matches(&dependency_id);
            assert(KnowledgeSection::clone_equivalent(&left[prior], &right[prior]));
            assert(left[prior].spec_id() == right[prior].spec_id());
        }
    }
    assert(required_section_has_kind(
        right, repository_inventory, KnowledgeSectionKind::RepositoryInventory)) by {
        let prior = choose |index: int| 0 <= index < left.len()
            && left[index].spec_id().spec_matches(&repository_inventory)
            && left[index].spec_kind() == KnowledgeSectionKind::RepositoryInventory;
        assert(KnowledgeSection::clone_equivalent(&left[prior], &right[prior]));
    }
    assert(required_section_has_kind(
        right, relevant_file_map, KnowledgeSectionKind::RelevantFileMap)) by {
        let prior = choose |index: int| 0 <= index < left.len()
            && left[index].spec_id().spec_matches(&relevant_file_map)
            && left[index].spec_kind() == KnowledgeSectionKind::RelevantFileMap;
        assert(KnowledgeSection::clone_equivalent(&left[prior], &right[prior]));
    }
    assert(required_section_has_kind(
        right, requirement_ledger, KnowledgeSectionKind::LiteralRequirementLedger)) by {
        let prior = choose |index: int| 0 <= index < left.len()
            && left[index].spec_id().spec_matches(&requirement_ledger)
            && left[index].spec_kind() == KnowledgeSectionKind::LiteralRequirementLedger;
        assert(KnowledgeSection::clone_equivalent(&left[prior], &right[prior]));
    }
}

} // verus!
