//! Exact fenced-lineage correlation validation.

use crate::model::concrete::identity::identifier_values_equal;
use crate::{
    LeaseError, ReconciliationCorrelation, ReconciliationDimension, ScopeDimension,
};
use vstd::prelude::*;

verus! {

pub(super) const fn validate_correlation(
    expected: ReconciliationCorrelation,
    actual: ReconciliationCorrelation,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => {
                correlation_error(expected, actual).is_none()
                    && crate::model::concrete::fencing::concrete_correlation_matches(
                        actual,
                        expected,
                    )
            }
            Err(error) => correlation_error(expected, actual) == Some(error),
        },
{
    proof {
        actual.reveal_exact_fields();
        expected.reveal_exact_fields();
    }
    let actual_workspace = *actual.scope.workspace.as_bytes();
    let expected_workspace = *expected.scope.workspace.as_bytes();
    let actual_resource = *actual.scope.resource.as_bytes();
    let expected_resource = *expected.scope.resource.as_bytes();
    let actual_environment = *actual.scope.environment.as_bytes();
    let expected_environment = *expected.scope.environment.as_bytes();
    let actual_actor = *actual.prior_holder.actor_id.as_bytes();
    let expected_actor = *expected.prior_holder.actor_id.as_bytes();
    let actual_session = *actual.prior_holder.session_id.as_bytes();
    let expected_session = *expected.prior_holder.session_id.as_bytes();
    assert(actual_workspace == actual.scope.workspace.spec_bytes());
    assert(expected_workspace == expected.scope.workspace.spec_bytes());
    assert(actual_resource == actual.scope.resource.spec_bytes());
    assert(expected_resource == expected.scope.resource.spec_bytes());
    assert(actual_environment == actual.scope.environment.spec_bytes());
    assert(expected_environment == expected.scope.environment.spec_bytes());
    assert(actual_actor == actual.prior_holder.actor_id.spec_bytes());
    assert(expected_actor == expected.prior_holder.actor_id.spec_bytes());
    assert(actual_session == actual.prior_holder.session_id.spec_bytes());
    assert(expected_session == expected.prior_holder.session_id.spec_bytes());
    let workspace_matches = identifier_values_equal(actual_workspace, expected_workspace);
    let resource_matches = identifier_values_equal(actual_resource, expected_resource);
    let environment_matches = identifier_values_equal(actual_environment, expected_environment);
    let actor_matches = identifier_values_equal(actual_actor, expected_actor);
    let session_matches = identifier_values_equal(actual_session, expected_session);
    let generation_matches =
        actual.fenced_generation.get() == expected.fenced_generation.get();
    if !workspace_matches {
        return Err(LeaseError::ReconciliationMismatch(ReconciliationDimension::Scope(
            ScopeDimension::Workspace,
        )));
    }
    if !resource_matches {
        return Err(LeaseError::ReconciliationMismatch(ReconciliationDimension::Scope(
            ScopeDimension::Resource,
        )));
    }
    if !environment_matches {
        return Err(LeaseError::ReconciliationMismatch(ReconciliationDimension::Scope(
            ScopeDimension::Environment,
        )));
    }
    if !generation_matches {
        return Err(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::FencedGeneration,
        ));
    }
    if !actor_matches || !session_matches {
        return Err(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::PriorHolder,
        ));
    }
    assert(crate::model::concrete_scope_matches(actual.scope, expected.scope));
    assert(expected.fenced_generation.spec_value() == actual.fenced_generation.spec_value());
    assert(crate::model::concrete_holder_matches(
        actual.prior_holder,
        expected.prior_holder,
    ));
    assert(crate::model::concrete::fencing::concrete_correlation_matches(
        actual,
        expected,
    ));
    assert(correlation_error(expected, actual).is_none());
    Ok(())
}

pub(crate) open spec fn correlation_error(
    expected: ReconciliationCorrelation,
    actual: ReconciliationCorrelation,
) -> Option<LeaseError> {
    if !crate::model::concrete_identifier_matches(
        actual.spec_scope().workspace.spec_bytes(),
        expected.spec_scope().workspace.spec_bytes(),
    ) {
        Some(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::Scope(ScopeDimension::Workspace),
        ))
    } else if !crate::model::concrete_identifier_matches(
        actual.spec_scope().resource.spec_bytes(),
        expected.spec_scope().resource.spec_bytes(),
    ) {
        Some(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::Scope(ScopeDimension::Resource),
        ))
    } else if !crate::model::concrete_identifier_matches(
        actual.spec_scope().environment.spec_bytes(),
        expected.spec_scope().environment.spec_bytes(),
    ) {
        Some(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::Scope(ScopeDimension::Environment),
        ))
    } else if actual.spec_fenced_generation().spec_value()
        != expected.spec_fenced_generation().spec_value()
    {
        Some(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::FencedGeneration,
        ))
    } else if !crate::model::concrete_holder_matches(
        actual.spec_prior_holder(),
        expected.spec_prior_holder(),
    ) {
        Some(LeaseError::ReconciliationMismatch(
            ReconciliationDimension::PriorHolder,
        ))
    } else {
        None
    }
}

} // verus!
