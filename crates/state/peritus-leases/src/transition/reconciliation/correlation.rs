//! Exact fenced-lineage correlation validation.

use crate::model::concrete::identity::identifier_values_equal;
use crate::{LeaseError, ReconciliationCorrelation, ReconciliationDimension, ScopeDimension};

pub(super) const fn validate_correlation(
    expected: ReconciliationCorrelation,
    actual: ReconciliationCorrelation,
) -> Result<(), LeaseError> {
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
    let workspace_matches = identifier_values_equal(actual_workspace, expected_workspace);
    let resource_matches = identifier_values_equal(actual_resource, expected_resource);
    let environment_matches = identifier_values_equal(actual_environment, expected_environment);
    let actor_matches = identifier_values_equal(actual_actor, expected_actor);
    let session_matches = identifier_values_equal(actual_session, expected_session);
    let generation_matches = actual.fenced_generation.get() == expected.fenced_generation.get();
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
        return Err(LeaseError::ReconciliationMismatch(ReconciliationDimension::FencedGeneration));
    }
    if !actor_matches || !session_matches {
        return Err(LeaseError::ReconciliationMismatch(ReconciliationDimension::PriorHolder));
    }
    Ok(())
}
