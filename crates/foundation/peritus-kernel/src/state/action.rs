//! Action lifecycle state.

use crate::ActionAuthorizationWitness;
use peritus_policy::ActorRole;
use peritus_types::{ActionId, ActorId, EnvironmentId, Sha256Digest, TurnId};

#[doc = " Lifecycle phase of one proposed or executed action."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActionPhase {
    #[doc = " Proposed but not authorized."]
    Proposed,
    #[doc = " Exact B1 capability use was checked."]
    Authorized,
    #[doc = " An effect adapter was logically dispatched."]
    Dispatched,
    #[doc = " The action completed successfully."]
    Succeeded,
    #[doc = " The action failed."]
    Failed,
    #[doc = " The action was cancelled before dispatch."]
    Cancelled,
}
impl ActionPhase {
    #[doc = " Returns whether no later action transition is legal."]
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
#[doc = " Current state of one exact action."]
#[derive(Debug, Eq, PartialEq)]
pub struct ActionState {
    id: ActionId,
    turn_id: TurnId,
    digest: Sha256Digest,
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    phase: ActionPhase,
    authorization: Option<ActionAuthorizationWitness>,
}
impl ActionState {
    pub(crate) fn clone_sequence(actions: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(actions.len());
        let mut index = 0;
        while index < actions.len() {
            result.push(actions[index].clone());
            index += 1;
        }
        result
    }
    pub(crate) const fn proposed(
        id: ActionId,
        turn_id: TurnId,
        digest: Sha256Digest,
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
    ) -> Self {
        Self {
            id,
            turn_id,
            digest,
            actor_id,
            role,
            environment_id,
            phase: ActionPhase::Proposed,
            authorization: None,
        }
    }
    #[doc = " Returns the action identity."]
    #[must_use]
    pub const fn id(&self) -> ActionId {
        self.id
    }
    #[doc = " Returns the parent turn."]
    #[must_use]
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    #[doc = " Returns the digest of canonical action bytes."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the acting principal."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the stable actor role."]
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[doc = " Returns the execution environment."]
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    #[doc = " Returns the current phase."]
    #[must_use]
    pub const fn phase(&self) -> ActionPhase {
        self.phase
    }
    #[doc = " Returns the checked authorization witness, if authorized."]
    #[must_use]
    pub const fn authorization(&self) -> Option<&ActionAuthorizationWitness> {
        self.authorization.as_ref()
    }
    pub(crate) fn authorize(&mut self, witness: ActionAuthorizationWitness) {
        self.authorization = Some(witness);
        self.phase = ActionPhase::Authorized;
    }
    pub(crate) const fn set_phase(&mut self, phase: ActionPhase) {
        self.phase = phase;
    }
}
impl Clone for ActionState {
    fn clone(&self) -> Self {
        if let Some(witness) = &self.authorization {
            return Self {
                id: self.id,
                turn_id: self.turn_id,
                digest: self.digest,
                actor_id: self.actor_id,
                role: self.role,
                environment_id: self.environment_id,
                phase: self.phase,
                authorization: Some(witness.clone()),
            };
        }
        Self {
            id: self.id,
            turn_id: self.turn_id,
            digest: self.digest,
            actor_id: self.actor_id,
            role: self.role,
            environment_id: self.environment_id,
            phase: self.phase,
            authorization: None,
        }
    }
}
