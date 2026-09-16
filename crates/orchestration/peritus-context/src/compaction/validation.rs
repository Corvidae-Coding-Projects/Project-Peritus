//! Transactional validation and construction of compacted nodes.

use super::{CompactionPolicy, CompactionProposal, ValidatedCompaction, ValidatedSource};
use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextGraph, ContextLimits,
    ContextNode, ContextNodeId, ContextNodeMetadata, ContextPlan, Provenance, RequirementMode,
    RoleVisibility, SelectionReason, TrustClass,
};
use core::cmp::Ordering;
use peritus_policy::ActorRole;
use peritus_role::ContextClass;
use vstd::prelude::*;

verus! {

/// Validates source selection, visibility, bounds, digests, protection, lineage, and savings.
///
/// # Errors
///
/// Returns a typed rejection without producing a partial derived node.
#[allow(clippy::too_many_lines, reason = "transactional validation keeps rejection order explicit")]
#[allow(
    clippy::branches_sharing_code,
    reason = "branch-local proof facts establish the two source-ID recurrence cases"
)]
pub fn validate_compaction(
    graph: &ContextGraph,
    plan: &ContextPlan,
    proposal: &CompactionProposal,
    policy: CompactionPolicy,
    limits: ContextLimits,
) -> (result: Result<ValidatedCompaction, ContextError>)
    ensures match result {
        Ok(validated) => validated.spec_matches_proposal(proposal, policy),
        Err(_) => true,
    },
{
    if proposal.policy_id != policy.id() {
        return Err(ContextError::node(
            ContextErrorKind::CompactionPolicyMismatch,
            proposal.node_id,
        ));
    }
    let mut self_range_index = 0;
    while self_range_index < proposal.source_ranges.len()
        invariant self_range_index <= proposal.source_ranges.len(),
        decreases proposal.source_ranges.len() - self_range_index,
    {
        if proposal.source_ranges[self_range_index].source_id() == proposal.node_id {
            return Err(ContextError::nodes(
                ContextErrorKind::CompactionSourceCycle,
                proposal.node_id,
                proposal.node_id,
            ));
        }
        self_range_index += 1;
    }
    if graph.node(proposal.node_id).is_some() {
        return Err(ContextError::node(
            ContextErrorKind::CompactionNodeExists,
            proposal.node_id,
        ));
    }

    let mut dependencies: Vec<ContextNodeId> = Vec::new();
    let mut visibility: Option<Vec<ActorRole>> = None;
    let mut context_class: Option<ContextClass> = None;
    let mut replaced_tokens = 0u64;
    let mut requirement = RequirementMode::Optional;
    let mut all_trusted = true;
    let mut sources = Vec::new();
    let mut range_index = 0;
    proof {
        reveal(CompactionProposal::source_ids);
    }
    while range_index < proposal.source_ranges.len()
        invariant
            range_index <= proposal.source_ranges.len(),
            dependencies@ == CompactionProposal::source_ids(
                proposal.spec_source_ranges().take(range_index as int),
            ),
            sources@.map_values(|source: ValidatedSource|
                source.spec_node().spec_id()) == dependencies@,
        decreases proposal.source_ranges.len() - range_index,
    {
        let range = proposal.source_ranges[range_index];
        let Some(source) = graph.node(range.source_id()) else {
            return Err(ContextError::nodes(
                ContextErrorKind::MissingCompactionSource,
                proposal.node_id,
                range.source_id(),
            ));
        };
        if source.digest() != range.source_digest() {
            return Err(ContextError::nodes(
                ContextErrorKind::DigestMismatch,
                proposal.node_id,
                range.source_id(),
            ));
        }
        if range.end() > source.content().len() as u64 {
            return Err(ContextError::nodes(
                ContextErrorKind::InvalidSourceRange,
                proposal.node_id,
                range.source_id(),
            ));
        }
        if !plan.contains(range.source_id()) {
            return Err(ContextError::nodes(
                ContextErrorKind::CompactionSourceNotSelected,
                proposal.node_id,
                range.source_id(),
            ));
        }
        if !source.visibility().contains(plan.role_profile().actor_role())
            || !plan.role_profile().context().visible().contains(source.context_class())
        {
            return Err(ContextError::nodes(
                ContextErrorKind::HiddenCompactionSource,
                proposal.node_id,
                range.source_id(),
            ));
        }
        if source.content_kind().is_compaction_protected() {
            return Err(ContextError::nodes(
                ContextErrorKind::ProtectedCompactionSource,
                proposal.node_id,
                range.source_id(),
            ));
        }
        if let Some(class) = context_class {
            if class != source.context_class() {
                return Err(ContextError::nodes(
                    ContextErrorKind::IncompatibleCompactionClasses,
                    proposal.node_id,
                    range.source_id(),
                ));
            }
        } else {
            context_class = Some(source.context_class());
        }

        let ghost prior_ids = dependencies@;
        let ghost prior_sources = sources@;
        let ghost prefix = proposal.spec_source_ranges().take(range_index as int);
        let ghost next_prefix = proposal.spec_source_ranges().take(range_index as int + 1);
        proof {
            assert(proposal.spec_source_ranges()[range_index as int] == range);
            assert(prefix.len() == range_index);
            assert(CompactionProposal::source_ids(prefix) == prior_ids);
            assert(prefix.push(range) =~= next_prefix);
            assert(next_prefix.drop_last() =~= prefix);
            assert(next_prefix.last() == range);
        }
        let new_source = dependencies.is_empty()
            || !dependencies[dependencies.len() - 1].matches(&range.source_id());
        if new_source {
            dependencies.push(range.source_id());
            replaced_tokens = replaced_tokens
                .checked_add(source.token_estimate())
                .ok_or_else(|| ContextError::node(ContextErrorKind::ArithmeticOverflow, proposal.node_id))?;
            visibility = Some(intersect_visibility(visibility, source.visibility().roles()));
            if source.requirement().precedence() > requirement.precedence() {
                requirement = source.requirement();
            }
            if source.trust() != TrustClass::Trusted {
                all_trusted = false;
            }
            let required = source.requirement() != RequirementMode::Optional
                || selected_as_required(plan, range.source_id());
            let source_node = source.clone();
            proof {
                reveal(ContextNode::clone_equivalent);
                let source_id = source.spec_id();
                let range_id = range.spec_source_id();
                ContextNodeId::matches_implies_equal(&source_id, &range_id);
                assert(source.spec_id() == range.spec_source_id());
                assert(source_node.spec_id() == source.spec_id());
                assert(prior_ids.len() == 0
                    || !prior_ids.last().spec_matches(&range.spec_source_id()));
            }
            sources.push(ValidatedSource { node: source_node, required });
            proof {
                reveal(ValidatedSource::spec_node);
                assert(sources@ == prior_sources.push(sources@.last()));
                assert(sources@.last().spec_node().spec_id() == range.spec_source_id());
                assert(dependencies@ == prior_ids.push(range.spec_source_id()));
                reveal(CompactionProposal::source_ids);
                assert(CompactionProposal::source_ids(next_prefix)
                    == prior_ids.push(range.spec_source_id()));
            };
        } else {
            proof {
                assert(prior_ids.len() > 0);
                assert(prior_ids.last().spec_matches(&range.spec_source_id()));
                assert(dependencies@ == prior_ids);
                assert(sources@ == prior_sources);
                reveal(CompactionProposal::source_ids);
                assert(CompactionProposal::source_ids(next_prefix) == prior_ids);
            };
        }
        range_index += 1;
    }
    if proposal.token_estimate >= replaced_tokens {
        return Err(ContextError::node_numbers(
            ContextErrorKind::CompactionNotSmaller,
            proposal.node_id,
            replaced_tokens.saturating_sub(1),
            proposal.token_estimate,
        ));
    }
    let Some(roles) = visibility else {
        return Err(ContextError::node(ContextErrorKind::EmptyCollection, proposal.node_id));
    };
    let visibility = RoleVisibility::new(roles, limits)?;
    let Some(context_class) = context_class else {
        return Err(ContextError::node(ContextErrorKind::EmptyCollection, proposal.node_id));
    };
    let metadata = ContextNodeMetadata::new(
        proposal.node_id,
        Provenance::DerivedCompaction,
        AuthorityClass::NonAuthoritative,
        TrustClass::Untrusted,
        context_class,
        ContentKind::DerivedSummary,
        proposal.token_estimate,
        proposal.recency_sequence,
        requirement,
        proposal.priority,
        visibility,
        dependencies,
        limits,
    )?;
    let metadata = if all_trusted && policy.preserves_trust() {
        metadata.preserve_compaction_trust()
    } else {
        metadata
    };
    proof {
        assert(metadata.spec_id() == proposal.spec_node_id());
        assert(metadata.spec_provenance() == Provenance::DerivedCompaction);
        assert(metadata.spec_authority() == AuthorityClass::NonAuthoritative);
        assert(metadata.spec_content_kind() == ContentKind::DerivedSummary);
        assert(metadata.spec_token_estimate() == proposal.spec_token_estimate());
    }
    let content = proposal.content.clone();
    let node = ContextNode::new(metadata, content);
    let ghost expected_source_ids = proposal.spec_source_ids();
    proof {
        assert(proposal.spec_source_ranges().take(range_index as int)
            =~= proposal.spec_source_ranges());
        assert(sources@.map_values(|source: ValidatedSource|
            source.spec_node().spec_id()) == expected_source_ids);
    }
    let validated = ValidatedCompaction {
        node,
        policy_id: policy.id(),
        source_ranges: proposal.source_ranges.clone(),
        replaced_tokens,
        sources,
    };
    proof {
        reveal(ValidatedCompaction::spec_node);
        reveal(ValidatedCompaction::spec_policy_id);
        reveal(ValidatedCompaction::spec_source_ranges);
        reveal(ValidatedCompaction::spec_replaced_tokens);
        reveal(ValidatedCompaction::spec_source_ids);
        reveal(ValidatedSource::spec_node);
        assert(validated.spec_node().spec_metadata().spec_id() == proposal.spec_node_id());
        assert(validated.spec_node().spec_metadata().spec_provenance()
            == Provenance::DerivedCompaction);
        assert(validated.spec_node().spec_metadata().spec_authority()
            == AuthorityClass::NonAuthoritative);
        assert(validated.spec_node().spec_metadata().spec_content_kind()
            == ContentKind::DerivedSummary);
        assert(validated.spec_node().spec_metadata().spec_token_estimate()
            == proposal.spec_token_estimate());
        assert(crate::ContextContent::clone_equivalent(
            &proposal.spec_content(),
            &validated.spec_node().spec_content(),
        ));
        assert(validated.spec_policy_id() == policy.spec_id());
        assert(validated.spec_source_ranges() == proposal.spec_source_ranges());
        assert(validated.spec_source_ids() == expected_source_ids);
        assert(validated.spec_is_strict_reduction());
        assert(validated.spec_matches_proposal(proposal, policy));
    }
    Ok(validated)
}

fn selected_as_required(plan: &ContextPlan, source_id: ContextNodeId) -> bool {
    let selected_entries = plan.selected();
    let mut index = 0;
    while index < selected_entries.len()
        invariant index <= selected_entries.len(),
        decreases selected_entries.len() - index,
    {
        let selected = selected_entries[index];
        if selected.node_id() == source_id {
            return matches!(
                selected.reason(),
                SelectionReason::RequiredRoot | SelectionReason::RequiredDependency
            );
        }
        index += 1;
    }
    false
}

fn intersect_visibility(current: Option<Vec<ActorRole>>, next: &[ActorRole]) -> Vec<ActorRole> {
    let Some(current) = current else {
        let mut copied = Vec::with_capacity(next.len());
        let mut index = 0;
        while index < next.len()
            invariant index <= next.len(),
            decreases next.len() - index,
        {
            copied.push(next[index]);
            index += 1;
        }
        return copied;
    };
    let mut intersection = Vec::new();
    let mut left = 0;
    let mut right = 0;
    while left < current.len() && right < next.len()
        invariant
            left <= current.len(),
            right <= next.len(),
        decreases (current.len() - left) + (next.len() - right),
    {
        match current[left].cmp(&next[right]) {
            Ordering::Equal => {
                intersection.push(current[left]);
                left += 1;
                right += 1;
            }
            Ordering::Less => left += 1,
            Ordering::Greater => right += 1,
        }
    }
    intersection
}

} // verus!
