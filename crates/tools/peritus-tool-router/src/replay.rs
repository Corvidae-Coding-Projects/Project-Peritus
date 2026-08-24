//! Bounded one-use and terminal replay ledger.

use std::collections::BTreeMap;

use peritus_tool_protocol::{IdempotencySemantics, PreparedToolCall, ReplayIdentity, ToolResult};
use peritus_types::ActionId;

use crate::{DispatchOutcome, ReplayDisposition, RouterError, RouterErrorKind};

pub enum ReplayState {
    Reserved {
        identity: ReplayIdentity,
    },
    Active {
        identity: ReplayIdentity,
    },
    Terminal {
        identity: ReplayIdentity,
        idempotency: IdempotencySemantics,
        result: Box<ToolResult>,
    },
    Indeterminate {
        identity: ReplayIdentity,
    },
}

pub struct ReplayLedger {
    entries: BTreeMap<ActionId, ReplayState>,
    capacity: usize,
}

impl ReplayLedger {
    pub(crate) const fn new(capacity: usize) -> Self {
        Self { entries: BTreeMap::new(), capacity }
    }

    pub(crate) fn inspect(
        &self,
        prepared: &PreparedToolCall,
    ) -> Result<Option<DispatchOutcome>, RouterError> {
        let Some(state) = self.entries.get(&prepared.call().action_id()) else {
            return Ok(None);
        };
        let expected = prepared.replay_identity();
        let actual = match state {
            ReplayState::Reserved { identity }
            | ReplayState::Active { identity }
            | ReplayState::Terminal { identity, .. }
            | ReplayState::Indeterminate { identity } => *identity,
        };
        if actual != expected {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "inspect tool replay",
                "action identity was reused with different bound bytes",
            ));
        }
        Ok(Some(match state {
            ReplayState::Reserved { .. } | ReplayState::Active { .. } => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::Active)
            }
            ReplayState::Terminal {
                idempotency: IdempotencySemantics::ReplayTerminal,
                result,
                ..
            } => DispatchOutcome::Replayed(result.as_ref().clone()),
            ReplayState::Terminal { .. } => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::NonIdempotentTerminal)
            }
            ReplayState::Indeterminate { .. } => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::Indeterminate)
            }
        }))
    }

    pub(crate) fn reserve(&mut self, prepared: &PreparedToolCall) -> Result<(), RouterError> {
        if self.entries.len() >= self.capacity {
            return Err(RouterError::new(
                RouterErrorKind::Capacity,
                "reserve tool invocation",
                "bounded replay ledger is full",
            ));
        }
        let previous = self.entries.insert(
            prepared.call().action_id(),
            ReplayState::Reserved { identity: prepared.replay_identity() },
        );
        if previous.is_some() {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "reserve tool invocation",
                "action was concurrently reserved",
            ));
        }
        Ok(())
    }

    pub(crate) fn mark_active(&mut self, prepared: &PreparedToolCall) {
        self.entries.insert(
            prepared.call().action_id(),
            ReplayState::Active { identity: prepared.replay_identity() },
        );
    }

    pub(crate) fn complete(&mut self, prepared: &PreparedToolCall, result: ToolResult) {
        self.entries.insert(
            prepared.call().action_id(),
            ReplayState::Terminal {
                identity: prepared.replay_identity(),
                idempotency: prepared.descriptor().idempotency(),
                result: Box::new(result),
            },
        );
    }

    pub(crate) fn indeterminate(&mut self, prepared: &PreparedToolCall) {
        self.entries.insert(
            prepared.call().action_id(),
            ReplayState::Indeterminate { identity: prepared.replay_identity() },
        );
    }
}
