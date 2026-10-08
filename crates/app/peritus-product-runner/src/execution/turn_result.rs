//! Completed developer-turn values shared by writer and fixer phases.

/// Host-observed work retained independently from the model's terminal report.
pub struct HostTurnEvidence {
    /// Number of provider tool calls observed in terminal provider responses.
    pub tool_calls: u64,
    /// Conversation revision under which the retained work was performed.
    pub conversation_revision: u64,
    /// Bounded structured command requests and observations from the host.
    pub verification_evidence: String,
    /// Successful, explicitly classified developer commands retained for delivery evidence.
    pub(crate) successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

/// One applied developer turn.
pub struct AppliedWrite {
    /// Task-level summary returned by this developer turn.
    pub summary: String,
    /// Concrete command or steps for running the result.
    pub run_instructions: String,
    /// Number of actual developer-tool calls executed.
    pub tool_calls: u64,
    /// Conversation revision incorporated by the turn.
    pub conversation_revision: u64,
    /// Bounded structured command requests and observations from this developer turn.
    pub verification_evidence: String,
    /// Successful, explicitly classified developer commands retained for delivery evidence.
    pub(crate) successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

/// Terminal state of one developer turn.
pub enum AppliedTurn {
    /// The model performed work and returned a terminal summary.
    Applied(AppliedWrite),
    /// The model requires one material answer before continuing.
    Waiting {
        /// Direct question for the user.
        question: String,
        /// Conversation revision on which the question was based.
        conversation_revision: u64,
        /// Host facts observed before the model asked its material question.
        host: HostTurnEvidence,
    },
    /// The model or provider did not produce an acceptable terminal report after host work.
    Rejected {
        /// Exact terminal failure used for settlement and recovery guidance.
        error: crate::ProductRunnerError,
        /// Host facts retained without treating the model report as authoritative.
        host: HostTurnEvidence,
    },
}
