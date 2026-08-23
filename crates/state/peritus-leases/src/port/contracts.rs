//! Exact request construction, untrusted echo validation, and resolution contracts.

use super::{
    equality, LeaseCasExpectation, LeaseCasObservation, LeaseCasRequest, LeaseCasResolution,
    ProtocolViolation, ValidatedAppliedLeaseClaim,
};
use crate::{LeaseAggregate, LeaseTransition, LeaseTransitionRecord};
use peritus_types::{CommandId, WorkspaceId};
use vstd::prelude::*;

verus! {

impl LeaseCasRequest {
    pub(super) open spec fn exact_transition_fields(
        &self,
        transition: &LeaseTransition,
    ) -> bool {
        self.workspace_id == transition.record.scope.workspace
            && self.expected == match transition.record.before_version {
                Some(version) => LeaseCasExpectation::Version(version),
                None => LeaseCasExpectation::Absent,
            }
            && self.command_id == transition.record.command_id
            && self.planned == transition.next
            && self.record == transition.record
    }

    /// Returns whether this request exactly consumes every field of an accepted logical plan.
    pub closed spec fn spec_matches_transition(
        &self,
        transition: &LeaseTransition,
    ) -> bool {
        self.exact_transition_fields(transition)
    }

    /// Creates the exact CAS request for an accepted logical transition.
    #[must_use]
    pub fn from_transition(transition: LeaseTransition) -> (request: Self)
        ensures request.spec_matches_transition(&transition),
    {
        let (planned, record) = transition.into_cas_parts();
        let expected = expectation_from_version(record.before_version);
        Self {
            workspace_id: record.scope.workspace,
            expected,
            command_id: record.command_id,
            planned,
            record,
        }
    }

    /// Validates one bounded adapter observation against the complete submitted request.
    ///
    /// An exactly matching claim remains unprivileged; only C0 may establish durable commit.
    #[must_use]
    pub fn resolve_observation(
        &self,
        observation: LeaseCasObservation,
    ) -> (resolution: LeaseCasResolution)
        ensures concrete_cas_resolution(self, &observation, &resolution),
    {
        match observation {
            LeaseCasObservation::ClaimedApplied { workspace_id, command_id } => {
                if exact_identity_matches(self, workspace_id, command_id) {
                    LeaseCasResolution::ClaimedApplied(ValidatedAppliedLeaseClaim {
                        workspace_id,
                        command_id,
                    })
                } else {
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                }
            }
            LeaseCasObservation::Conflict {
                workspace_id,
                command_id,
                observed,
            } => {
                if exact_identity_matches(self, workspace_id, command_id) {
                    LeaseCasResolution::Conflict(observed)
                } else {
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                }
            }
            LeaseCasObservation::DefinitelyNotApplied { workspace_id, command_id } => {
                if exact_identity_matches(self, workspace_id, command_id) {
                    LeaseCasResolution::DefinitelyNotApplied
                } else {
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                }
            }
            LeaseCasObservation::Indeterminate { workspace_id, command_id } => {
                if exact_identity_matches(self, workspace_id, command_id) {
                    LeaseCasResolution::Indeterminate
                } else {
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                }
            }
            LeaseCasObservation::ProtocolInvalid { workspace_id, command_id, violation } => {
                if exact_identity_matches(self, workspace_id, command_id) {
                    LeaseCasResolution::ProtocolInvalid(violation)
                } else {
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                }
            }
        }
    }

    /// Compares authoritative stored fields with every field of this submitted request.
    ///
    /// This is the C0 refinement boundary for a state owner that has already reconstructed a
    /// checked aggregate snapshot. A positive match is still not a receipt or effect permit.
    #[must_use]
    pub fn authoritative_fields_match(
        &self,
        workspace_id: WorkspaceId,
        expected: LeaseCasExpectation,
        command_id: CommandId,
        planned: &LeaseAggregate,
        record: &LeaseTransitionRecord,
    ) -> (matches: bool)
        ensures matches == self.spec_authoritative_fields_match(
            workspace_id, expected, command_id, planned, record,
        ),
    {
        equality::bytes16_equal(
            *workspace_id.as_bytes(),
            *self.workspace_id.as_bytes(),
        ) && equality::expectations_equal(expected, self.expected)
            && equality::bytes16_equal(
                *command_id.as_bytes(),
                *self.command_id.as_bytes(),
            )
            && equality::aggregates_equal(planned, &self.planned)
            && equality::records_equal(record, &self.record)
    }

    /// Exact full-plan relation required before C0 may issue durable commit authority.
    pub closed spec fn spec_authoritative_fields_match(
        &self,
        workspace_id: WorkspaceId,
        expected: LeaseCasExpectation,
        command_id: CommandId,
        planned: &LeaseAggregate,
        record: &LeaseTransitionRecord,
    ) -> bool {
        exact_authoritative_fields(
            self,
            workspace_id,
            expected,
            command_id,
            planned,
            record,
        )
    }
}

const fn expectation_from_version(
    before_version: Option<peritus_types::RevisionNumber>,
) -> (expected: LeaseCasExpectation)
    ensures
        expected == match before_version {
            Some(version) => LeaseCasExpectation::Version(version),
            None => LeaseCasExpectation::Absent,
        },
{
    let Some(version) = before_version else {
        return LeaseCasExpectation::Absent;
    };
    LeaseCasExpectation::Version(version)
}

pub(super) open spec fn exact_authoritative_fields(
    request: &LeaseCasRequest,
    workspace_id: WorkspaceId,
    expected: LeaseCasExpectation,
    command_id: CommandId,
    planned: &LeaseAggregate,
    record: &LeaseTransitionRecord,
) -> bool {
    equality::bytes16_match(
        workspace_id.spec_bytes(),
        request.workspace_id.spec_bytes(),
    ) && equality::expectation_fields_match(expected, request.expected)
        && equality::bytes16_match(
            command_id.spec_bytes(),
            request.command_id.spec_bytes(),
        )
        && equality::aggregate_fields_match(planned, &request.planned)
        && equality::record_fields_match(record, &request.record)
}

impl ValidatedAppliedLeaseClaim {
    /// Returns whether this claim carries the request's exact aggregate and command identity.
    pub closed spec fn spec_matches_request(&self, request: &LeaseCasRequest) -> bool {
        identity_fields_match(request, self.workspace_id, self.command_id)
    }
}

pub(super) open spec fn identity_fields_match(
    request: &LeaseCasRequest,
    workspace_id: WorkspaceId,
    command_id: CommandId,
) -> bool {
    equality::bytes16_match(
        workspace_id.spec_bytes(),
        request.workspace_id.spec_bytes(),
    ) && equality::bytes16_match(
        command_id.spec_bytes(),
        request.command_id.spec_bytes(),
    )
}

fn exact_identity_matches(
    request: &LeaseCasRequest,
    workspace_id: WorkspaceId,
    command_id: CommandId,
) -> (matches: bool)
    ensures matches == identity_fields_match(request, workspace_id, command_id),
{
    equality::bytes16_equal(
        *workspace_id.as_bytes(),
        *request.workspace_id.as_bytes(),
    ) && equality::bytes16_equal(
        *command_id.as_bytes(),
        *request.command_id.as_bytes(),
    )
}

pub(super) open spec fn exact_cas_resolution(
    request: &LeaseCasRequest,
    observation: &LeaseCasObservation,
    resolution: &LeaseCasResolution,
) -> bool {
    match observation {
        LeaseCasObservation::ClaimedApplied { workspace_id, command_id } => {
            if identity_fields_match(request, *workspace_id, *command_id) {
                match resolution {
                    LeaseCasResolution::ClaimedApplied(validated) =>
                        validated.workspace_id == *workspace_id
                            && validated.command_id == *command_id
                            && validated.spec_matches_request(request),
                    _ => false,
                }
            } else {
                matches!(
                    resolution,
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                )
            }
        }
        LeaseCasObservation::Conflict { workspace_id, command_id, observed } => {
            if identity_fields_match(request, *workspace_id, *command_id) {
                match resolution {
                    LeaseCasResolution::Conflict(actual) => actual == observed,
                    _ => false,
                }
            } else {
                matches!(
                    resolution,
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                )
            }
        }
        LeaseCasObservation::DefinitelyNotApplied { workspace_id, command_id } => {
            if identity_fields_match(request, *workspace_id, *command_id) {
                matches!(resolution, LeaseCasResolution::DefinitelyNotApplied)
            } else {
                matches!(
                    resolution,
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                )
            }
        }
        LeaseCasObservation::Indeterminate { workspace_id, command_id } => {
            if identity_fields_match(request, *workspace_id, *command_id) {
                matches!(resolution, LeaseCasResolution::Indeterminate)
            } else {
                matches!(
                    resolution,
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                )
            }
        }
        LeaseCasObservation::ProtocolInvalid {
            workspace_id,
            command_id,
            violation,
        } => {
            if identity_fields_match(request, *workspace_id, *command_id) {
                match resolution {
                    LeaseCasResolution::ProtocolInvalid(actual) => actual == *violation,
                    _ => false,
                }
            } else {
                matches!(
                    resolution,
                    LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
                )
            }
        }
    }
}

/// Exact fail-closed interpretation of one complete untrusted adapter observation.
pub closed spec fn concrete_cas_resolution(
    request: &LeaseCasRequest,
    observation: &LeaseCasObservation,
    resolution: &LeaseCasResolution,
) -> bool {
    exact_cas_resolution(request, observation, resolution)
}

} // verus!
