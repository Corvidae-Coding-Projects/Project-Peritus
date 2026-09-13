//! Verified semantic clones for the closed command vocabulary.

use super::{AgentCommandKind, ProviderEventRecord};
use vstd::prelude::*;

verus! {

impl ProviderEventRecord {
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.cursor == right.cursor
            && left.event_digest == right.event_digest
            && left.output_bytes == right.output_bytes
            && left.duplicate == right.duplicate
            && left.encoded_envelope@ == right.encoded_envelope@
    }
}

impl AgentCommandKind {
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        match (left, right) {
        (AgentCommandKind::ContextPrepared(left), AgentCommandKind::ContextPrepared(right)) => {
            left == right
        },
        (
            AgentCommandKind::ModelRequestStarted {
                call_id: left_call,
                request_digest: left_digest,
            },
            AgentCommandKind::ModelRequestStarted {
                call_id: right_call,
                request_digest: right_digest,
            },
        ) => left_call == right_call && left_digest == right_digest,
        (
            AgentCommandKind::ProviderEventObserved(left),
            AgentCommandKind::ProviderEventObserved(right),
        ) => ProviderEventRecord::clone_equivalent(left, right),
        (
            AgentCommandKind::ProviderRetryScheduled(left),
            AgentCommandKind::ProviderRetryScheduled(right),
        ) => left == right,
        (
            AgentCommandKind::ToolCallsProposed {
                terminal: left_terminal,
                proposals: left_proposals,
            },
            AgentCommandKind::ToolCallsProposed {
                terminal: right_terminal,
                proposals: right_proposals,
            },
        ) => left_terminal == right_terminal
            && crate::ToolProposal::sequence_clone_equivalent(
                left_proposals@,
                right_proposals@,
            ),
        (
            AgentCommandKind::CompletionProposed {
                terminal: left_terminal,
                proposal: left_proposal,
            },
            AgentCommandKind::CompletionProposed {
                terminal: right_terminal,
                proposal: right_proposal,
            },
        ) => left_terminal == right_terminal
            && crate::CompletionProposal::clone_equivalent(left_proposal, right_proposal),
        (AgentCommandKind::AuthorizationStarted, AgentCommandKind::AuthorizationStarted) => true,
        (
            AgentCommandKind::ToolAuthorized {
                ordinal: left_ordinal,
                authority_digest: left_digest,
            },
            AgentCommandKind::ToolAuthorized {
                ordinal: right_ordinal,
                authority_digest: right_digest,
            },
        ) => left_ordinal == right_ordinal && left_digest == right_digest,
        (
            AgentCommandKind::ToolDenied { ordinal: left_ordinal, result: left_result },
            AgentCommandKind::ToolDenied { ordinal: right_ordinal, result: right_result },
        ) => left_ordinal == right_ordinal
            && crate::ToolResultRecord::clone_equivalent(left_result, right_result),
        (AgentCommandKind::ToolExecutionStarted, AgentCommandKind::ToolExecutionStarted) => true,
        (
            AgentCommandKind::ToolDispatched { ordinal: left },
            AgentCommandKind::ToolDispatched { ordinal: right },
        )
        | (
            AgentCommandKind::ToolActivated { ordinal: left },
            AgentCommandKind::ToolActivated { ordinal: right },
        ) => left == right,
        (
            AgentCommandKind::ToolProgressObserved {
                ordinal: left_ordinal,
                sequence: left_sequence,
                progress_digest: left_digest,
            },
            AgentCommandKind::ToolProgressObserved {
                ordinal: right_ordinal,
                sequence: right_sequence,
                progress_digest: right_digest,
            },
        ) => left_ordinal == right_ordinal
            && left_sequence == right_sequence
            && left_digest == right_digest,
        (
            AgentCommandKind::ToolCompleted { ordinal: left_ordinal, result: left_result },
            AgentCommandKind::ToolCompleted { ordinal: right_ordinal, result: right_result },
        ) => left_ordinal == right_ordinal
            && crate::ToolResultRecord::clone_equivalent(left_result, right_result),
        (AgentCommandKind::ResultRecordingStarted, AgentCommandKind::ResultRecordingStarted) => {
            true
        },
        (
            AgentCommandKind::ResultsRecorded { transcript_digest: left },
            AgentCommandKind::ResultsRecorded { transcript_digest: right },
        ) => left == right,
        (AgentCommandKind::Paused, AgentCommandKind::Paused)
        | (AgentCommandKind::CancellationRequested, AgentCommandKind::CancellationRequested)
        | (AgentCommandKind::CancellationFinished, AgentCommandKind::CancellationFinished)
        | (AgentCommandKind::CompletionCommitted, AgentCommandKind::CompletionCommitted) => true,
        (
            AgentCommandKind::Resumed { recovery_checked: left },
            AgentCommandKind::Resumed { recovery_checked: right },
        ) => left == right,
        (AgentCommandKind::Failed(left), AgentCommandKind::Failed(right))
        | (AgentCommandKind::Exhausted(left), AgentCommandKind::Exhausted(right)) => left == right,
        _ => false,
    }
}
}

impl Clone for ProviderEventRecord {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        let encoded_envelope = self.encoded_envelope.clone();
        proof {
            assert(encoded_envelope@ =~= self.encoded_envelope@);
        }
        Self {
            cursor: self.cursor,
            event_digest: self.event_digest,
            output_bytes: self.output_bytes,
            duplicate: self.duplicate,
            encoded_envelope,
        }
    }
}

impl Clone for AgentCommandKind {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        match self {
            Self::ContextPrepared(context) => Self::ContextPrepared(*context),
            Self::ModelRequestStarted { call_id, request_digest } => Self::ModelRequestStarted {
                call_id: *call_id,
                request_digest: *request_digest,
            },
            Self::ProviderEventObserved(record) => Self::ProviderEventObserved(record.clone()),
            Self::ProviderRetryScheduled(record) => Self::ProviderRetryScheduled(*record),
            Self::ToolCallsProposed { terminal, proposals } => Self::ToolCallsProposed {
                terminal: *terminal,
                proposals: crate::tools::clone_tool_proposals(proposals),
            },
            Self::CompletionProposed { terminal, proposal } => Self::CompletionProposed {
                terminal: *terminal,
                proposal: proposal.clone(),
            },
            Self::AuthorizationStarted => Self::AuthorizationStarted,
            Self::ToolAuthorized { ordinal, authority_digest } => Self::ToolAuthorized {
                ordinal: *ordinal,
                authority_digest: *authority_digest,
            },
            Self::ToolDenied { ordinal, result } => {
                Self::ToolDenied { ordinal: *ordinal, result: result.clone() }
            },
            Self::ToolExecutionStarted => Self::ToolExecutionStarted,
            Self::ToolDispatched { ordinal } => Self::ToolDispatched { ordinal: *ordinal },
            Self::ToolActivated { ordinal } => Self::ToolActivated { ordinal: *ordinal },
            Self::ToolProgressObserved { ordinal, sequence, progress_digest } => {
                Self::ToolProgressObserved {
                    ordinal: *ordinal,
                    sequence: *sequence,
                    progress_digest: *progress_digest,
                }
            },
            Self::ToolCompleted { ordinal, result } => {
                Self::ToolCompleted { ordinal: *ordinal, result: result.clone() }
            },
            Self::ResultRecordingStarted => Self::ResultRecordingStarted,
            Self::ResultsRecorded { transcript_digest } => {
                Self::ResultsRecorded { transcript_digest: *transcript_digest }
            },
            Self::Paused => Self::Paused,
            Self::Resumed { recovery_checked } => {
                Self::Resumed { recovery_checked: *recovery_checked }
            },
            Self::CancellationRequested => Self::CancellationRequested,
            Self::CancellationFinished => Self::CancellationFinished,
            Self::Failed(failure) => Self::Failed(failure.clone()),
            Self::Exhausted(failure) => Self::Exhausted(failure.clone()),
            Self::CompletionCommitted => Self::CompletionCommitted,
        }
    }
}

} // verus!
