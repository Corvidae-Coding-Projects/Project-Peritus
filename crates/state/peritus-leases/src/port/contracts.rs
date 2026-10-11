//! Exact request construction, untrusted echo validation, and resolution contracts.

use super::{
    LeaseCasExpectation, LeaseCasObservation, LeaseCasRequest, LeaseCasResolution,
    ProtocolViolation, ValidatedAppliedLeaseClaim, equality,
};
use crate::{LeaseAggregate, LeaseTransition, LeaseTransitionRecord};
use peritus_types::{CommandId, WorkspaceId};

impl LeaseCasRequest {
    #[doc = " Creates the exact CAS request for an accepted logical transition."]
    #[must_use]
    pub fn from_transition(transition: LeaseTransition) -> Self {
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
    #[doc = " Validates one bounded adapter observation against the complete submitted request."]
    #[doc = ""]
    #[doc = " An exactly matching claim remains unprivileged; only C0 may establish durable commit."]
    #[must_use]
    pub fn resolve_observation(&self, observation: LeaseCasObservation) -> LeaseCasResolution {
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
            LeaseCasObservation::Conflict { workspace_id, command_id, observed } => {
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
    #[doc = " Compares authoritative stored fields with every field of this submitted request."]
    #[doc = ""]
    #[doc = " This is the C0 refinement boundary for a state owner that has already reconstructed a"]
    #[doc = " checked aggregate snapshot. A positive match is still not a receipt or effect permit."]
    #[must_use]
    pub fn authoritative_fields_match(
        &self,
        workspace_id: WorkspaceId,
        expected: LeaseCasExpectation,
        command_id: CommandId,
        planned: &LeaseAggregate,
        record: &LeaseTransitionRecord,
    ) -> bool {
        equality::bytes16_equal(*workspace_id.as_bytes(), *self.workspace_id.as_bytes())
            && equality::expectations_equal(expected, self.expected)
            && equality::bytes16_equal(*command_id.as_bytes(), *self.command_id.as_bytes())
            && equality::aggregates_equal(planned, &self.planned)
            && equality::records_equal(record, &self.record)
    }
}
const fn expectation_from_version(
    before_version: Option<peritus_types::RevisionNumber>,
) -> LeaseCasExpectation {
    let Some(version) = before_version else {
        return LeaseCasExpectation::Absent;
    };
    LeaseCasExpectation::Version(version)
}
impl ValidatedAppliedLeaseClaim {}
fn exact_identity_matches(
    request: &LeaseCasRequest,
    workspace_id: WorkspaceId,
    command_id: CommandId,
) -> bool {
    equality::bytes16_equal(*workspace_id.as_bytes(), *request.workspace_id.as_bytes())
        && equality::bytes16_equal(*command_id.as_bytes(), *request.command_id.as_bytes())
}
