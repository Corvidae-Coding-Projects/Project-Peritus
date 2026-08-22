//! A tool-branded wrapper over protocol-neutral exact scripts.

use crate::{ExpectedCall, ObservedCall, ScriptIncomplete, ScriptViolation, ScriptedCalls};
use std::error::Error;
use std::fmt;

/// A fake-tool script protocol failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolScriptError {
    violation: ScriptViolation,
}

impl ToolScriptError {
    /// Returns the underlying generic script violation.
    #[must_use]
    pub const fn violation(self) -> ScriptViolation {
        self.violation
    }

    /// Returns a stable tool-branded diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        "PERITUS-TEST-TOOL-001"
    }
}

impl fmt::Display for ToolScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake tool script failed: {}", self.violation)
    }
}

impl Error for ToolScriptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.violation)
    }
}

/// A protocol-neutral fake tool with exact one-shot outcomes.
///
/// This type intentionally implements no production tool trait. Simulated tool errors belong in
/// the caller-owned `Outcome` type and remain distinct from [`ToolScriptError`].
#[derive(Debug)]
pub struct FakeTool<Request, Outcome> {
    script: ScriptedCalls<Request, Outcome>,
}

impl<Request, Outcome> FakeTool<Request, Outcome> {
    /// Creates a tool script in iterator order.
    #[must_use]
    pub fn new(steps: impl IntoIterator<Item = ExpectedCall<Request, Outcome>>) -> Self {
        Self { script: ScriptedCalls::new(steps) }
    }

    /// Returns all observed requests.
    #[must_use]
    pub fn observed(&self) -> &[ObservedCall<Request>] {
        self.script.observed()
    }

    /// Returns the number of unconsumed outcomes.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.script.remaining()
    }

    /// Verifies that every expected request was matched.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptIncomplete`] with the exact remaining count.
    pub fn verify_complete(&self) -> Result<(), ScriptIncomplete> {
        self.script.verify_complete()
    }
}

impl<Request: PartialEq, Outcome> FakeTool<Request, Outcome> {
    /// Returns the next exact caller-owned outcome for `request`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolScriptError`] when the call violates the script.
    pub fn outcome_for(&mut self, request: Request) -> Result<Outcome, ToolScriptError> {
        self.script.respond(request).map_err(|violation| ToolScriptError { violation })
    }
}
