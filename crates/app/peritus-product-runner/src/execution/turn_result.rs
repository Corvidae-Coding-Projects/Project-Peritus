//! Completed developer-turn values shared by writer and fixer phases.

/// One applied developer turn.
pub struct AppliedWrite {
    /// Task-level summary returned by this developer turn.
    pub summary: String,
    /// Concrete command or steps for running the result.
    pub run_instructions: String,
    /// Number of actual developer-tool calls executed.
    pub tool_calls: u32,
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
    },
}
