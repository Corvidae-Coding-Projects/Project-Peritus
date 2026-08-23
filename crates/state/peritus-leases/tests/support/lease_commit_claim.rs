//! Reusable adversarial scripts for the durable lease compare-and-swap boundary.

use super::FixtureIds;
use peritus_leases::{
    LeaseAggregate, LeaseCasExpectation, LeaseCasObservation, LeaseCasPort, LeaseCasRequest,
    LeasePortFailure, ObservedLeaseState, ProtocolViolation,
};
use peritus_types::{CommandId, WorkspaceId};
use std::collections::VecDeque;

/// Named adversarial and crash-boundary fixture families required by AC18.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseCommitClaimFixture {
    ForgedWorkspaceIdentity,
    ForgedCommandIdentity,
    MalformedObservation,
    StaleSnapshot,
    ConflictingCommandReuse,
    DuplicateClaim,
    CorruptSnapshot,
    IndeterminateObservation,
    FailureBeforeCommit,
    FailureAfterCommitBeforeAck,
}

/// One observed call into a scripted state-owner adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseCasCall {
    Compare { workspace_id: WorkspaceId, expected: LeaseCasExpectation, command_id: CommandId },
    Resolve { workspace_id: WorkspaceId, command_id: CommandId },
}

#[derive(Debug)]
struct ScriptedStep {
    expected_call: LeaseCasCall,
    result: Result<LeaseCasObservation, LeasePortFailure>,
}

/// Move-preserving, call-checking state-owner adapter used by adversarial tests.
#[derive(Debug)]
pub struct ScriptedLeaseCas {
    steps: VecDeque<ScriptedStep>,
    calls: Vec<LeaseCasCall>,
}

impl ScriptedLeaseCas {
    /// Builds one complete named script, including any mandatory recovery call.
    pub fn for_fixture(
        fixture: LeaseCommitClaimFixture,
        request: &LeaseCasRequest,
        ids: &FixtureIds,
        observed: Option<LeaseAggregate>,
    ) -> Self {
        let compare = compare_call(request);
        let exact_workspace = request.workspace_id();
        let exact_command = request.command_id();
        let claimed_applied = || LeaseCasObservation::ClaimedApplied {
            workspace_id: exact_workspace,
            command_id: exact_command,
        };
        let protocol_invalid = |violation| LeaseCasObservation::ProtocolInvalid {
            workspace_id: exact_workspace,
            command_id: exact_command,
            violation,
        };
        let steps = match fixture {
            LeaseCommitClaimFixture::ForgedWorkspaceIdentity => vec![step(
                compare,
                Ok(LeaseCasObservation::ClaimedApplied {
                    workspace_id: ids.other_workspace,
                    command_id: exact_command,
                }),
            )],
            LeaseCommitClaimFixture::ForgedCommandIdentity => vec![step(
                compare,
                Ok(LeaseCasObservation::ClaimedApplied {
                    workspace_id: exact_workspace,
                    command_id: super::command(0xF0),
                }),
            )],
            LeaseCommitClaimFixture::MalformedObservation => {
                vec![step(compare, Ok(protocol_invalid(ProtocolViolation::MalformedObservation)))]
            }
            LeaseCommitClaimFixture::StaleSnapshot => vec![step(
                compare,
                Ok(LeaseCasObservation::Conflict {
                    workspace_id: exact_workspace,
                    command_id: exact_command,
                    observed: ObservedLeaseState::Present(Box::new(
                        observed.expect("stale-snapshot fixture requires an observed aggregate"),
                    )),
                }),
            )],
            LeaseCommitClaimFixture::ConflictingCommandReuse => vec![step(
                compare,
                Ok(protocol_invalid(ProtocolViolation::AuthoritativePlanMismatch)),
            )],
            LeaseCommitClaimFixture::DuplicateClaim => {
                vec![step(compare, Ok(claimed_applied())), step(compare, Ok(claimed_applied()))]
            }
            LeaseCommitClaimFixture::CorruptSnapshot => {
                vec![step(compare, Ok(protocol_invalid(ProtocolViolation::InvalidSnapshot)))]
            }
            LeaseCommitClaimFixture::IndeterminateObservation => vec![
                step(
                    compare,
                    Ok(LeaseCasObservation::Indeterminate {
                        workspace_id: exact_workspace,
                        command_id: exact_command,
                    }),
                ),
                step(
                    resolve_call(request),
                    Ok(LeaseCasObservation::DefinitelyNotApplied {
                        workspace_id: exact_workspace,
                        command_id: exact_command,
                    }),
                ),
            ],
            LeaseCommitClaimFixture::FailureBeforeCommit => {
                vec![step(compare, Err(LeasePortFailure::Unavailable))]
            }
            LeaseCommitClaimFixture::FailureAfterCommitBeforeAck => vec![
                step(compare, Err(LeasePortFailure::Indeterminate)),
                step(resolve_call(request), Ok(claimed_applied())),
            ],
        };
        Self { steps: steps.into(), calls: Vec::new() }
    }

    /// Returns the exact adapter-call trace accumulated so far.
    pub fn calls(&self) -> &[LeaseCasCall] {
        &self.calls
    }

    /// Returns whether the caller consumed the complete recovery script.
    pub fn is_complete(&self) -> bool {
        self.steps.is_empty()
    }

    fn run_step(
        &mut self,
        actual_call: LeaseCasCall,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        let scripted = self.steps.pop_front().expect("unexpected extra lease CAS call");
        assert_eq!(
            scripted.expected_call, actual_call,
            "lease CAS call did not preserve exact request identity"
        );
        self.calls.push(actual_call);
        scripted.result
    }
}

impl LeaseCasPort for ScriptedLeaseCas {
    fn compare_and_swap(
        &mut self,
        request: &LeaseCasRequest,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        self.run_step(compare_call(request))
    }

    fn resolve_command(
        &mut self,
        workspace_id: WorkspaceId,
        command_id: CommandId,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        self.run_step(LeaseCasCall::Resolve { workspace_id, command_id })
    }
}

const fn step(
    expected_call: LeaseCasCall,
    result: Result<LeaseCasObservation, LeasePortFailure>,
) -> ScriptedStep {
    ScriptedStep { expected_call, result }
}

const fn compare_call(request: &LeaseCasRequest) -> LeaseCasCall {
    LeaseCasCall::Compare {
        workspace_id: request.workspace_id(),
        expected: request.expected(),
        command_id: request.command_id(),
    }
}

const fn resolve_call(request: &LeaseCasRequest) -> LeaseCasCall {
    LeaseCasCall::Resolve { workspace_id: request.workspace_id(), command_id: request.command_id() }
}
