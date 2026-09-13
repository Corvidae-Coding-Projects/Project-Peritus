//! Exact admission and lookup for node role visibility.

use crate::{ContextError, ContextErrorKind, ContextLimits};
use peritus_policy::ActorRole;
use vstd::prelude::*;

verus! {

/// Nonempty, canonically ordered set of B1 roles allowed to see a node.
#[derive(Debug, Eq, PartialEq)]
pub struct RoleVisibility {
    roles: Vec<ActorRole>,
}

impl RoleVisibility {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// Logical view of every admitted role.
    pub closed spec fn spec_roles(&self) -> Seq<ActorRole> { self.roles@ }

    /// First canonical-order failure at or after `index`.
    pub open spec fn first_order_error(
        roles: Seq<ActorRole>,
        index: nat,
    ) -> Option<ContextErrorKind>
        decreases roles.len() - index,
    {
        if index >= roles.len() {
            None
        } else if roles[index as int - 1].spec_rank() == roles[index as int].spec_rank() {
            Some(ContextErrorKind::DuplicateValue)
        } else if roles[index as int - 1].spec_rank() > roles[index as int].spec_rank() {
            Some(ContextErrorKind::NonCanonicalOrder)
        } else {
            Self::first_order_error(roles, index + 1)
        }
    }

    /// Strict canonical role order retained by every constructed value.
    pub open spec fn roles_canonical(roles: Seq<ActorRole>) -> bool {
        Self::first_order_error(roles, 1).is_none()
    }

    /// Intrinsic nonempty canonical shape.
    pub open spec fn spec_well_formed(&self) -> bool {
        self.spec_roles().len() > 0 && Self::roles_canonical(self.spec_roles())
    }

    /// Exact constructor admission for the supplied allocation bound.
    pub open spec fn inputs_valid(roles: Seq<ActorRole>, limits: ContextLimits) -> bool {
        0 < roles.len() <= limits.spec_max_visibility_roles()
            && Self::roles_canonical(roles)
    }

    /// Exact constructor failure and precedence.
    pub open spec fn construction_error(
        roles: Seq<ActorRole>,
        limits: ContextLimits,
        error: ContextError,
    ) -> bool {
        if roles.len() == 0 {
            error.spec_is_plain(ContextErrorKind::EmptyCollection)
        } else if roles.len() > limits.spec_max_visibility_roles() {
            error.spec_is_numbers(
                ContextErrorKind::TooManyVisibilityRoles,
                limits.spec_max_visibility_roles() as u64,
                roles.len() as u64,
            )
        } else {
            Self::first_order_error(roles, 1) == Some(error.spec_kind())
                && error.spec_node_id().is_none()
                && error.spec_related_id().is_none()
                && error.spec_expected().is_none()
                && error.spec_actual().is_none()
        }
    }

    /// Exact identity membership at or after `index`.
    pub open spec fn roles_contain_from(
        roles: Seq<ActorRole>,
        role: ActorRole,
        index: nat,
    ) -> bool
        decreases roles.len() - index,
    {
        index < roles.len()
            && (roles[index as int].spec_rank() == role.spec_rank()
                || Self::roles_contain_from(roles, role, index + 1))
    }

    /// Exact identity membership in the supplied role sequence.
    pub open spec fn roles_contain(roles: Seq<ActorRole>, role: ActorRole) -> bool {
        Self::roles_contain_from(roles, role, 0)
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_roles() == right.spec_roles()
    }

    /// Checks nonemptiness, the configured bound, uniqueness, and canonical ordering.
    ///
    /// # Errors
    ///
    /// Returns the exact first typed collection or bound error.
    pub fn new(
        roles: Vec<ActorRole>,
        limits: ContextLimits,
    ) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() == Self::inputs_valid(roles@, limits),
            match result {
                Ok(value) => value.spec_roles() == roles@ && value.spec_well_formed(),
                Err(error) => Self::construction_error(roles@, limits, error),
            },
    {
        if roles.is_empty() {
            return Err(ContextError::plain(ContextErrorKind::EmptyCollection));
        }
        if roles.len() > limits.max_visibility_roles() {
            return Err(ContextError::with_numbers(
                ContextErrorKind::TooManyVisibilityRoles,
                limits.max_visibility_roles() as u64,
                roles.len() as u64,
            ));
        }
        validate_order(&roles)?;
        Ok(Self { roles })
    }

    /// Borrows the canonical roles.
    #[must_use]
    pub const fn roles(&self) -> (roles: &[ActorRole])
        ensures roles@ == self.spec_roles(),
    { self.roles.as_slice() }

    /// Returns whether the exact B1 role is present.
    #[must_use]
    pub fn contains(&self, role: ActorRole) -> (contains: bool)
        ensures contains == Self::roles_contain(self.spec_roles(), role),
    {
        proof {
            use_type_invariant(self);
            reveal(RoleVisibility::spec_roles);
        }
        let target = role_rank(role);
        let mut index = 0;
        while index < self.roles.len()
            invariant
                0 <= index <= self.roles.len(),
                target as int == role.spec_rank(),
                Self::roles_contain_from(self.spec_roles(), role, 0)
                    == Self::roles_contain_from(self.spec_roles(), role, index as nat),
            decreases self.roles.len() - index,
        {
            if role_rank(self.roles[index]) == target {
                return true;
            }
            index += 1;
        }
        false
    }
}

fn validate_order(roles: &[ActorRole]) -> (result: Result<(), ContextError>)
    requires roles@.len() > 0,
    ensures
        result.is_ok() == RoleVisibility::first_order_error(roles@, 1).is_none(),
        match result {
            Ok(()) => true,
            Err(error) => RoleVisibility::first_order_error(roles@, 1) == Some(error.spec_kind())
                && error.spec_node_id().is_none()
                && error.spec_related_id().is_none()
                && error.spec_expected().is_none()
                && error.spec_actual().is_none(),
        },
{
    let mut index = 1;
    while index < roles.len()
        invariant
            1 <= index <= roles.len(),
            RoleVisibility::first_order_error(roles@, 1)
                == RoleVisibility::first_order_error(roles@, index as nat),
        decreases roles.len() - index,
    {
        let previous = role_rank(roles[index - 1]);
        let current = role_rank(roles[index]);
        if previous == current {
            return Err(ContextError::plain(ContextErrorKind::DuplicateValue));
        }
        if previous > current {
            return Err(ContextError::plain(ContextErrorKind::NonCanonicalOrder));
        }
        index += 1;
    }
    Ok(())
}

impl Clone for RoleVisibility {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self { roles: self.roles.clone() }
    }
}

const fn role_rank(role: ActorRole) -> (rank: u8)
    ensures rank as int == role.spec_rank(),
{
    match role {
        ActorRole::Writer => 0,
        ActorRole::Fixer => 1,
        ActorRole::Reviewer => 2,
        ActorRole::Evaluator => 3,
        ActorRole::GateRunner => 4,
        ActorRole::Orchestrator => 5,
        ActorRole::EvolutionAgent => 6,
        ActorRole::HumanAuthority => 7,
        ActorRole::DaemonService => 8,
        ActorRole::ProviderToolWorker => 9,
        ActorRole::Plugin => 10,
    }
}

} // verus!
