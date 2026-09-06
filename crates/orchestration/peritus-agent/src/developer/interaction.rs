//! Optional live user-input and public activity boundary for developer execution.

use super::DeveloperLoopError;

/// One atomically captured governing conversation revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeveloperInput {
    /// Monotonic durable user-input revision.
    pub revision: u64,
    /// Current user-visible conversation, without private reasoning or credentials.
    pub conversation: String,
}

/// Public execution activity, separate from the raw durable provider trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperActivity<'a> {
    /// A model request is about to start.
    ModelStarted,
    /// Public assistant text received from a provider, never a reasoning delta.
    Text(&'a [u8]),
    /// One exact tool call is about to execute.
    ToolStarted { name: &'a str, arguments: &'a str },
    /// A tool completed with a bounded, user-visible result.
    ToolFinished { name: &'a str, output: &'a str, is_error: bool },
    /// A tool call was not executed because newer user input superseded it.
    ToolSkipped { name: &'a str },
}

/// Daemon-owned live input and observation port; it cannot grant tool authority.
pub trait DeveloperInteraction: Send + Sync {
    /// Atomically captures current input. Failure stops rather than using a stale snapshot.
    ///
    /// # Errors
    /// Returns a durable-input or synchronization failure.
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError>;

    /// Records the revision included in the next provider request.
    ///
    /// # Errors
    /// Returns a persistence failure; no request is sent after a failed acknowledgement.
    fn applied(&self, revision: u64) -> Result<(), DeveloperLoopError>;

    /// Records safe public activity independently of raw trace storage.
    ///
    /// # Errors
    /// Returns an observation failure rather than claiming invisible successful progress.
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError>;
}
