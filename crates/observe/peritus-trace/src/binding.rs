//! Immutable binding from observations to existing Peritus domain identities.

use crate::{TraceError, TraceErrorKind};
use peritus_types::{
    ActionId, AttemptId, GateExecutionId, GateId, ProviderProfileId, RunId, SessionId,
    Sha256Digest, TurnId,
};

/// Exact authoritative-entity identities correlated by one span.
///
/// The value is descriptive only. It contains no receipt, permission, lease, or transition method.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CausalBinding {
    session_id: SessionId,
    run_id: Option<RunId>,
    attempt_id: Option<AttemptId>,
    turn_id: Option<TurnId>,
    action_id: Option<ActionId>,
    provider_profile_id: Option<ProviderProfileId>,
    tool_descriptor_digest: Option<Sha256Digest>,
    gate_id: Option<GateId>,
    gate_execution_id: Option<GateExecutionId>,
}

impl CausalBinding {
    /// Creates a session-level binding.
    #[must_use]
    pub const fn session(session_id: SessionId) -> Self {
        Self {
            session_id,
            run_id: None,
            attempt_id: None,
            turn_id: None,
            action_id: None,
            provider_profile_id: None,
            tool_descriptor_digest: None,
            gate_id: None,
            gate_execution_id: None,
        }
    }

    /// Refines the binding to one run.
    #[must_use]
    pub const fn with_run(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    /// Refines a run binding to one attempt.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no run is bound.
    pub const fn with_attempt(mut self, attempt_id: AttemptId) -> Result<Self, TraceError> {
        if self.run_id.is_none() {
            return Err(invalid("attempt binding requires a run"));
        }
        self.attempt_id = Some(attempt_id);
        Ok(self)
    }

    /// Refines an attempt binding to one turn.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no attempt is bound.
    pub const fn with_turn(mut self, turn_id: TurnId) -> Result<Self, TraceError> {
        if self.attempt_id.is_none() {
            return Err(invalid("turn binding requires an attempt"));
        }
        self.turn_id = Some(turn_id);
        Ok(self)
    }

    /// Refines a turn binding to one action.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no turn is bound.
    pub const fn with_action(mut self, action_id: ActionId) -> Result<Self, TraceError> {
        if self.turn_id.is_none() {
            return Err(invalid("action binding requires a turn"));
        }
        self.action_id = Some(action_id);
        Ok(self)
    }

    /// Adds one provider-profile correlation to a turn binding.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no turn is bound.
    pub const fn with_provider(
        mut self,
        provider_profile_id: ProviderProfileId,
    ) -> Result<Self, TraceError> {
        if self.turn_id.is_none() {
            return Err(invalid("provider binding requires a turn"));
        }
        self.provider_profile_id = Some(provider_profile_id);
        Ok(self)
    }

    /// Adds a canonical tool-descriptor digest to a turn binding.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no turn is bound.
    pub const fn with_tool(
        mut self,
        tool_descriptor_digest: Sha256Digest,
    ) -> Result<Self, TraceError> {
        if self.turn_id.is_none() {
            return Err(invalid("tool binding requires a turn"));
        }
        self.tool_descriptor_digest = Some(tool_descriptor_digest);
        Ok(self)
    }

    /// Adds one gate and gate-execution identity to an attempt binding.
    ///
    /// # Errors
    ///
    /// Returns an invalid-binding error when no attempt is bound.
    pub const fn with_gate(
        mut self,
        gate_id: GateId,
        gate_execution_id: GateExecutionId,
    ) -> Result<Self, TraceError> {
        if self.attempt_id.is_none() {
            return Err(invalid("gate binding requires an attempt"));
        }
        self.gate_id = Some(gate_id);
        self.gate_execution_id = Some(gate_execution_id);
        Ok(self)
    }

    #[allow(clippy::too_many_arguments, reason = "canonical decoding keeps every option explicit")]
    pub(crate) const fn from_parts(
        session_id: SessionId,
        run_id: Option<RunId>,
        attempt_id: Option<AttemptId>,
        turn_id: Option<TurnId>,
        action_id: Option<ActionId>,
        provider_profile_id: Option<ProviderProfileId>,
        tool_descriptor_digest: Option<Sha256Digest>,
        gate_id: Option<GateId>,
        gate_execution_id: Option<GateExecutionId>,
    ) -> Result<Self, TraceError> {
        let valid = (attempt_id.is_none() || run_id.is_some())
            && (turn_id.is_none() || attempt_id.is_some())
            && (action_id.is_none() || turn_id.is_some())
            && (provider_profile_id.is_none() || turn_id.is_some())
            && (tool_descriptor_digest.is_none() || turn_id.is_some())
            && (gate_id.is_none() == gate_execution_id.is_none())
            && (gate_execution_id.is_none() || attempt_id.is_some());
        if !valid {
            return Err(invalid("causal binding hierarchy is invalid"));
        }
        Ok(Self {
            session_id,
            run_id,
            attempt_id,
            turn_id,
            action_id,
            provider_profile_id,
            tool_descriptor_digest,
            gate_id,
            gate_execution_id,
        })
    }

    /// Returns the session identity.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
    /// Returns the run identity, when bound.
    #[must_use]
    pub const fn run_id(self) -> Option<RunId> {
        self.run_id
    }
    /// Returns the attempt identity, when bound.
    #[must_use]
    pub const fn attempt_id(self) -> Option<AttemptId> {
        self.attempt_id
    }
    /// Returns the turn identity, when bound.
    #[must_use]
    pub const fn turn_id(self) -> Option<TurnId> {
        self.turn_id
    }
    /// Returns the action identity, when bound.
    #[must_use]
    pub const fn action_id(self) -> Option<ActionId> {
        self.action_id
    }
    /// Returns the provider-profile identity, when bound.
    #[must_use]
    pub const fn provider_profile_id(self) -> Option<ProviderProfileId> {
        self.provider_profile_id
    }
    /// Returns the canonical tool-descriptor digest, when bound.
    #[must_use]
    pub const fn tool_descriptor_digest(self) -> Option<Sha256Digest> {
        self.tool_descriptor_digest
    }
    /// Returns the gate identity, when bound.
    #[must_use]
    pub const fn gate_id(self) -> Option<GateId> {
        self.gate_id
    }
    /// Returns the gate-execution identity, when bound.
    #[must_use]
    pub const fn gate_execution_id(self) -> Option<GateExecutionId> {
        self.gate_execution_id
    }

    /// Returns whether this binding preserves every identity already fixed by `ancestor`.
    #[must_use]
    pub fn refines(self, ancestor: Self) -> bool {
        self.session_id == ancestor.session_id
            && option_refines(self.run_id, ancestor.run_id)
            && option_refines(self.attempt_id, ancestor.attempt_id)
            && option_refines(self.turn_id, ancestor.turn_id)
            && option_refines(self.action_id, ancestor.action_id)
            && option_refines(self.provider_profile_id, ancestor.provider_profile_id)
            && option_refines(self.tool_descriptor_digest, ancestor.tool_descriptor_digest)
            && option_refines(self.gate_id, ancestor.gate_id)
            && option_refines(self.gate_execution_id, ancestor.gate_execution_id)
    }
}

fn option_refines<T: Copy + PartialEq>(value: Option<T>, ancestor: Option<T>) -> bool {
    ancestor.is_none_or(|expected| value == Some(expected))
}

const fn invalid(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::InvalidBinding, "validate causal binding", detail)
}
