//! Transactional validation and construction of compacted nodes.

use super::{CompactionPolicy, CompactionProposal, ValidatedCompaction};
use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextGraph, ContextLimits,
    ContextNode, ContextNodeMetadata, ContextPlan, Provenance, RequirementMode, RoleVisibility,
    TrustClass,
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
pub fn validate_compaction(
    graph: &ContextGraph,
    plan: &ContextPlan,
    proposal: &CompactionProposal,
    policy: CompactionPolicy,
    limits: ContextLimits,
) -> Result<ValidatedCompaction, ContextError> {
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

    let mut dependencies = Vec::new();
    let mut visibility: Option<Vec<ActorRole>> = None;
    let mut context_class: Option<ContextClass> = None;
    let mut replaced_tokens = 0u64;
    let mut requirement = RequirementMode::Optional;
    let mut all_trusted = true;
    let mut range_index = 0;
    while range_index < proposal.source_ranges.len()
        invariant range_index <= proposal.source_ranges.len(),
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

        let new_source = dependencies.is_empty()
            || dependencies[dependencies.len() - 1] != range.source_id();
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
    Ok(ValidatedCompaction {
        node: ContextNode::new(metadata, proposal.content.clone()),
        policy_id: policy.id(),
        source_ranges: proposal.source_ranges.clone(),
        replaced_tokens,
    })
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
