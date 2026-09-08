//! Public conversation policy shared by ordinary and verification-only API surfaces.

/// Conversational policy; authorized Chat effects hand off to the shared production pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationMode {
    /// Answer directly or hand authorized changes to the existing design/writer/reviewer loop.
    Chat,
    /// Inspect and plan with read-only tools.
    Plan,
    /// Independently inspect and review with read-only tools.
    Review,
}
