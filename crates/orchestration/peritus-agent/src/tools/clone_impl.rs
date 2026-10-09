//! Verified semantic clones for retained tool data.

use super::{ToolProposal, ToolResultRecord};
use vstd::prelude::*;

verus! {

impl ToolProposal {
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.ordinal == right.ordinal
            && left.model_call_id == right.model_call_id
            && left.action_id == right.action_id
            && left.capability.spec_value() == right.capability.spec_value()
            && left.capability.spec_bytes() == right.capability.spec_bytes()
            && left.version == right.version
            && left.argument_digest == right.argument_digest
            && left.prepared_digest == right.prepared_digest
            && left.replay_identity == right.replay_identity
            && left.revision == right.revision
            && left.deadline == right.deadline
            && left.side_effect == right.side_effect
            && left.idempotency == right.idempotency
    }

    pub closed spec fn sequence_clone_equivalent(
        left: Seq<ToolProposal>,
        right: Seq<ToolProposal>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto]
                0 <= index < left.len()
                    ==> Self::clone_equivalent(&left[index], &right[index])
    }
}

impl ToolResultRecord {
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.status == right.status
            && left.result_digest == right.result_digest
            && left.model_visible_bytes == right.model_visible_bytes
            && left.evidence@ == right.evidence@
    }
}

impl Clone for ToolProposal {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            ordinal: self.ordinal,
            model_call_id: self.model_call_id,
            action_id: self.action_id,
            capability: self.capability.clone(),
            version: self.version,
            argument_digest: self.argument_digest,
            prepared_digest: self.prepared_digest,
            replay_identity: self.replay_identity,
            revision: self.revision,
            deadline: self.deadline,
            side_effect: self.side_effect,
            idempotency: self.idempotency,
        }
    }
}

impl Clone for ToolResultRecord {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        let evidence = self.evidence.clone();
        proof {
            assert(evidence@ =~= self.evidence@);
        }
        Self {
            status: self.status,
            result_digest: self.result_digest,
            model_visible_bytes: self.model_visible_bytes,
            evidence,
        }
    }
}

pub(super) fn clone_tool_proposals(
    proposals: &[ToolProposal],
) -> (result: Vec<ToolProposal>)
    ensures ToolProposal::sequence_clone_equivalent(proposals@, result@),
{
    let mut result = Vec::with_capacity(proposals.len());
    let mut index = 0;
    while index < proposals.len()
        invariant
            index <= proposals.len(),
            result@.len() == index,
            forall |prior: int| #![auto]
                0 <= prior < index ==> ToolProposal::clone_equivalent(
                    &proposals@[prior],
                    &result@[prior],
                ),
        decreases proposals.len() - index,
    {
        result.push(proposals[index].clone());
        index += 1;
    }
    result
}

} // verus!
