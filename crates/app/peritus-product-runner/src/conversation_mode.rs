//! Public conversation policy shared by ordinary and verification-only API surfaces.

/// Governing conversational tool and prompt policy, separate from strict build execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationMode {
    /// Follow the user's exact scope, including authorized workspace changes.
    Chat,
    /// Inspect and plan with read-only tools.
    Plan,
    /// Independently inspect and review with read-only tools.
    Review,
}
