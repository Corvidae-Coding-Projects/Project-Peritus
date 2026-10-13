//! Public deterministic evidence projection and exact retrieval used by production role turns.

pub use crate::developer_tools::ReviewerEvidenceSources as DeveloperEvidenceSources;
pub use crate::local_context::LocalContextHandle;
pub use crate::turn::writer_prompt::{DeveloperEvidencePrompt, project_developer_evidence};

/// Returns the exact historical source reader schema used by developer and reviewer turns.
///
/// # Errors
/// Rejects invalid bounded schema construction.
pub fn evidence_tool_definition()
-> Result<peritus_model_protocol::ToolDefinition, crate::ProductRunnerError> {
    crate::developer_tools::evidence_definition()
}
