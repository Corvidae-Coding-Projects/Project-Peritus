//! Verified semantic clones for the closed command vocabulary.

use super::{AgentCommandKind, ProviderEventRecord};

impl ProviderEventRecord {}
impl AgentCommandKind {}
impl Clone for ProviderEventRecord {
    fn clone(&self) -> Self {
        let encoded_envelope = self.encoded_envelope.clone();
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
    fn clone(&self) -> Self {
        match self {
            Self::ContextPrepared(context) => Self::ContextPrepared(*context),
            Self::ModelRequestStarted { call_id, request_digest } => {
                Self::ModelRequestStarted { call_id: *call_id, request_digest: *request_digest }
            }
            Self::ProviderEventObserved(record) => Self::ProviderEventObserved(record.clone()),
            Self::ProviderRetryScheduled(record) => Self::ProviderRetryScheduled(*record),
            Self::ToolCallsProposed { terminal, proposals } => Self::ToolCallsProposed {
                terminal: *terminal,
                proposals: crate::tools::clone_tool_proposals(proposals),
            },
            Self::CompletionProposed { terminal, proposal } => {
                Self::CompletionProposed { terminal: *terminal, proposal: proposal.clone() }
            }
            Self::AuthorizationStarted => Self::AuthorizationStarted,
            Self::ToolAuthorized { ordinal, authority_digest } => {
                Self::ToolAuthorized { ordinal: *ordinal, authority_digest: *authority_digest }
            }
            Self::ToolDenied { ordinal, result } => {
                Self::ToolDenied { ordinal: *ordinal, result: result.clone() }
            }
            Self::ToolExecutionStarted => Self::ToolExecutionStarted,
            Self::ToolDispatched { ordinal } => Self::ToolDispatched { ordinal: *ordinal },
            Self::ToolActivated { ordinal } => Self::ToolActivated { ordinal: *ordinal },
            Self::ToolProgressObserved { ordinal, sequence, progress_digest } => {
                Self::ToolProgressObserved {
                    ordinal: *ordinal,
                    sequence: *sequence,
                    progress_digest: *progress_digest,
                }
            }
            Self::ToolCompleted { ordinal, result } => {
                Self::ToolCompleted { ordinal: *ordinal, result: result.clone() }
            }
            Self::ResultRecordingStarted => Self::ResultRecordingStarted,
            Self::ResultsRecorded { transcript_digest } => {
                Self::ResultsRecorded { transcript_digest: *transcript_digest }
            }
            Self::Paused => Self::Paused,
            Self::Resumed { recovery_checked } => {
                Self::Resumed { recovery_checked: *recovery_checked }
            }
            Self::CancellationRequested => Self::CancellationRequested,
            Self::CancellationFinished => Self::CancellationFinished,
            Self::Failed(failure) => Self::Failed(failure.clone()),
            Self::Exhausted(failure) => Self::Exhausted(failure.clone()),
            Self::CompletionCommitted => Self::CompletionCommitted,
        }
    }
}
