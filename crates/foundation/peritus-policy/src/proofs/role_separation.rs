//! Proofs for non-configurable role separation (`INV-007`).

#[cfg(verus_only)]
use crate::{model, ActorRole, OperationClass};
use vstd::prelude::*;

verus! {

pub proof fn reviewer_cannot_mutate()
    ensures !model::role_permits(ActorRole::Reviewer, OperationClass::WorkspaceMutation),
{}

pub proof fn evaluator_cannot_mutate()
    ensures !model::role_permits(ActorRole::Evaluator, OperationClass::WorkspaceMutation),
{}

pub proof fn writer_cannot_accept_or_amend()
    ensures
        !model::role_permits(ActorRole::Writer, OperationClass::Acceptance),
        !model::role_permits(ActorRole::Writer, OperationClass::Waiver),
        !model::role_permits(ActorRole::Writer, OperationClass::PolicyAmendment),
        !model::role_permits(ActorRole::Writer, OperationClass::HarnessPromotion),
{}

pub proof fn fixer_cannot_accept_or_amend()
    ensures
        !model::role_permits(ActorRole::Fixer, OperationClass::Acceptance),
        !model::role_permits(ActorRole::Fixer, OperationClass::Waiver),
        !model::role_permits(ActorRole::Fixer, OperationClass::PolicyAmendment),
        !model::role_permits(ActorRole::Fixer, OperationClass::HarnessPromotion),
{}

pub proof fn orchestrator_has_no_raw_effect_authority()
    ensures !model::role_permits(ActorRole::Orchestrator, OperationClass::RawEffect),
{}

} // verus!
