use super::{
    AgentErrorCode, AgentRejection, RevisionTuple, ToolBatch, ToolOrdinal, ToolProposal,
    ToolSideEffect, ToolSlot, ToolSlotPhase, tool_error,
};
use std::collections::BTreeSet;

impl ToolBatch {
    pub(crate) fn new(
        proposals: Vec<ToolProposal>,
        revision: RevisionTuple,
        max: u16,
    ) -> Result<Self, AgentRejection> {
        if proposals.is_empty() || proposals.len() > usize::from(max) {
            return Err(tool_error(
                AgentErrorCode::InvalidLimit,
                "tool batch is empty or exceeds the turn limit",
            ));
        }
        let mut actions = BTreeSet::new();
        let mut calls = BTreeSet::new();
        let mut mutation_count = 0_u16;
        for (index, proposal) in proposals.iter().enumerate() {
            let expected = u16::try_from(index).map_err(|_| {
                tool_error(AgentErrorCode::InvalidLimit, "tool ordinal exceeds representation")
            })?;
            if proposal.ordinal.get() != expected
                || proposal.revision != revision
                || !actions.insert(proposal.action_id)
                || !calls.insert(proposal.model_call_id)
            {
                return Err(tool_error(
                    AgentErrorCode::NonCanonicalOrder,
                    "tool proposals are unordered, duplicated, or stale",
                ));
            }
            if proposal.side_effect == ToolSideEffect::Workspace {
                mutation_count += 1;
            }
        }
        if mutation_count > 0 && proposals.len() > 1 {
            return Err(tool_error(
                AgentErrorCode::InvalidTool,
                "workspace mutations must be serialized",
            ));
        }
        Ok(Self { slots: proposals.into_iter().map(ToolSlot::proposed).collect() })
    }

    pub(crate) fn slot_mut_for_read(
        &self,
        ordinal: ToolOrdinal,
    ) -> Result<ToolSlotPhase, AgentRejection> {
        self.slots
            .get(usize::from(ordinal.get()))
            .map(ToolSlot::phase)
            .ok_or_else(|| tool_error(AgentErrorCode::InvalidTool, "unknown tool ordinal"))
    }
}
