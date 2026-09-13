//! Action lifecycle state.

use crate::ActionAuthorizationWitness;
use peritus_policy::ActorRole;
use peritus_types::{ActionId, ActorId, EnvironmentId, Sha256Digest, TurnId};
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one proposed or executed action.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActionPhase {
    /// Proposed but not authorized.
    Proposed,
    /// Exact B1 capability use was checked.
    Authorized,
    /// An effect adapter was logically dispatched.
    Dispatched,
    /// The action completed successfully.
    Succeeded,
    /// The action failed.
    Failed,
    /// The action was cancelled before dispatch.
    Cancelled,
}

impl ActionPhase {
    /// Returns whether no later action transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Current state of one exact action.
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
    /// Exact semantic fields preserved when an action state is cloned.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.id == right.id
            && left.turn_id == right.turn_id
            && left.digest == right.digest
            && left.actor_id == right.actor_id
            && left.role == right.role
            && left.environment_id == right.environment_id
            && left.phase == right.phase
            && match (&left.authorization, &right.authorization) {
                (Some(left), Some(right)) =>
                    ActionAuthorizationWitness::clone_equivalent(left, right),
                (None, None) => true,
                _ => false,
            }
    }

    pub(crate) closed spec fn sequence_clone_equivalent(
        left: Seq<ActionState>,
        right: Seq<ActionState>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto]
                0 <= index < left.len()
                    ==> Self::clone_equivalent(&left[index], &right[index])
    }

    pub(crate) fn clone_sequence(actions: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_clone_equivalent(actions@, result@),
    {
        let mut result = Vec::with_capacity(actions.len());
        let mut index = 0;
        while index < actions.len()
            invariant
                index <= actions.len(),
                result@.len() == index,
                forall |prior: int| #![auto]
                    0 <= prior < index ==> Self::clone_equivalent(
                        &actions@[prior],
                        &result@[prior],
                    ),
            decreases actions.len() - index,
        {
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
    /// Returns the action identity.
    #[must_use]
    pub const fn id(&self) -> ActionId { self.id }
    /// Returns the parent turn.
    #[must_use]
    pub const fn turn_id(&self) -> TurnId { self.turn_id }
    /// Returns the digest of canonical action bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Returns the acting principal.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId { self.actor_id }
    /// Returns the stable actor role.
    #[must_use]
    pub const fn role(&self) -> ActorRole { self.role }
    /// Returns the execution environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId { self.environment_id }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(&self) -> ActionPhase { self.phase }
    /// Returns the checked authorization witness, if authorized.
    #[must_use]
    pub const fn authorization(&self) -> Option<&ActionAuthorizationWitness> {
        self.authorization.as_ref()
    }
    pub(crate) fn authorize(&mut self, witness: ActionAuthorizationWitness) {
        self.authorization = Some(witness);
        self.phase = ActionPhase::Authorized;
    }
    pub(crate) const fn set_phase(&mut self, phase: ActionPhase) { self.phase = phase; }
}

impl Clone for ActionState {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
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

} // verus!
