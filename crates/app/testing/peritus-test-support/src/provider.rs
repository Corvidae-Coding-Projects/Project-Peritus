//! A provider-branded wrapper over protocol-neutral exact scripts.

use crate::{ExpectedCall, ObservedCall, ScriptIncomplete, ScriptViolation, ScriptedCalls};
use std::error::Error;
use std::fmt;

/// A fake-provider script protocol failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderScriptError {
    violation: ScriptViolation,
}

impl ProviderScriptError {
    /// Returns the underlying generic script violation.
    #[must_use]
    pub const fn violation(self) -> ScriptViolation {
        self.violation
    }

    /// Returns a stable provider-branded diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        "PERITUS-TEST-PROVIDER-001"
    }
}

impl fmt::Display for ProviderScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake provider script failed: {}", self.violation)
    }
}

impl Error for ProviderScriptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.violation)
    }
}

/// A protocol-neutral fake provider with exact one-shot outcomes.
///
/// This type intentionally implements no production provider trait. `Outcome` may contain a
/// caller-owned stream or simulated provider error.
#[derive(Debug)]
pub struct FakeProvider<Request, Outcome> {
    script: ScriptedCalls<Request, Outcome>,
}

impl<Request, Outcome> FakeProvider<Request, Outcome> {
    /// Creates a provider script in iterator order.
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

impl<Request: PartialEq, Outcome> FakeProvider<Request, Outcome> {
    /// Returns the next exact outcome for `request`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderScriptError`] when the call violates the script.
    pub fn response_for(&mut self, request: Request) -> Result<Outcome, ProviderScriptError> {
        self.script.respond(request).map_err(|violation| ProviderScriptError { violation })
    }
}
