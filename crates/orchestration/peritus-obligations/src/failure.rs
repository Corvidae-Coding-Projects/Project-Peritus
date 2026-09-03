//! Closed failure ownership and legal next transitions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use vstd::prelude::*;

verus! {

/// Component that owns the cause of a failed attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureOwner {
    /// The candidate implementation violates a clear public contract.
    CandidateDefect,
    /// The public contract has one material ambiguity.
    ContractAmbiguity,
    /// The selected model provider failed independently of candidate quality.
    ProviderFailure,
    /// Harness-owned execution, storage, workspace, or orchestration infrastructure failed.
    HarnessInfrastructure,
    /// The external evaluator or oracle failed independently of the candidate.
    ExternalEvaluator,
}

/// Prior recovery state needed to select one legal next transition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FailureContext {
    ambiguity_question_used: bool,
    recovery_available: bool,
}

impl FailureContext {
    /// Creates explicit ambiguity and recovery state.
    #[must_use]
    pub const fn new(ambiguity_question_used: bool, recovery_available: bool) -> Self {
        Self { ambiguity_question_used, recovery_available }
    }

    /// Whether the one allowed material clarification has already been asked.
    #[must_use]
    pub const fn ambiguity_question_used(self) -> bool { self.ambiguity_question_used }

    /// Whether a typed recovery route is currently available.
    #[must_use]
    pub const fn recovery_available(self) -> bool { self.recovery_available }
}

/// Sole legal transition selected from failure ownership.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureDisposition {
    /// Start another fixer cycle for a candidate defect.
    RequestFixer,
    /// Ask the one material public-contract question.
    AskMaterialQuestion,
    /// Recover the provider without blaming or changing the candidate.
    RecoverProvider,
    /// Recover harness infrastructure without blaming or changing the candidate.
    RecoverHarness,
    /// Settle honestly without another fixer cycle.
    Settle,
}

impl FailureOwner {
    /// Selects the only legal next transition for this owner and context.
    #[must_use]
    pub const fn disposition(self, context: FailureContext) -> FailureDisposition {
        match self {
            Self::CandidateDefect => FailureDisposition::RequestFixer,
            Self::ContractAmbiguity if !context.ambiguity_question_used() => {
                FailureDisposition::AskMaterialQuestion
            }
            Self::ProviderFailure if context.recovery_available() => {
                FailureDisposition::RecoverProvider
            }
            Self::HarnessInfrastructure if context.recovery_available() => {
                FailureDisposition::RecoverHarness
            }
            Self::ContractAmbiguity
            | Self::ProviderFailure
            | Self::HarnessInfrastructure
            | Self::ExternalEvaluator => FailureDisposition::Settle,
        }
    }

    /// Whether this owner can ever authorize a fixer transition.
    #[must_use]
    pub const fn authorizes_fixer(self) -> (result: bool)
        ensures result == crate::verified::failure_authorizes_fixer_spec(self),
    {
        matches!(self, Self::CandidateDefect)
    }
}

} // verus!
