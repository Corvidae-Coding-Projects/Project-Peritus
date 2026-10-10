//! Verified semantic clones for retained tool data.

use super::{ToolProposal, ToolResultRecord};

impl ToolProposal {}
impl ToolResultRecord {}
impl Clone for ToolProposal {
    fn clone(&self) -> Self {
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
            authority_epoch: self.authority_epoch,
            side_effect: self.side_effect,
            idempotency: self.idempotency,
        }
    }
}
impl Clone for ToolResultRecord {
    fn clone(&self) -> Self {
        let evidence = self.evidence.clone();
        Self {
            status: self.status,
            result_digest: self.result_digest,
            model_visible_bytes: self.model_visible_bytes,
            evidence,
        }
    }
}
pub(super) fn clone_tool_proposals(proposals: &[ToolProposal]) -> Vec<ToolProposal> {
    let mut result = Vec::with_capacity(proposals.len());
    let mut index = 0;
    while index < proposals.len() {
        result.push(proposals[index].clone());
        index += 1;
    }
    result
}
