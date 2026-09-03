//! Closed failure-owner transition matrix.

use peritus_obligations::{FailureContext, FailureDisposition, FailureOwner};

#[test]
fn only_candidate_defects_request_a_fixer_cycle() {
    for owner in [
        FailureOwner::CandidateDefect,
        FailureOwner::ContractAmbiguity,
        FailureOwner::ProviderFailure,
        FailureOwner::HarnessInfrastructure,
        FailureOwner::ExternalEvaluator,
    ] {
        for context in [FailureContext::new(false, false), FailureContext::new(true, true)] {
            assert_eq!(owner.authorizes_fixer(), owner == FailureOwner::CandidateDefect,);
            assert_eq!(
                owner.disposition(context) == FailureDisposition::RequestFixer,
                owner == FailureOwner::CandidateDefect,
            );
        }
    }
}

#[test]
fn ambiguity_asks_once_and_non_code_failures_recover_or_settle() {
    assert_eq!(
        FailureOwner::ContractAmbiguity.disposition(FailureContext::new(false, false)),
        FailureDisposition::AskMaterialQuestion,
    );
    assert_eq!(
        FailureOwner::ContractAmbiguity.disposition(FailureContext::new(true, true)),
        FailureDisposition::Settle,
    );
    assert_eq!(
        FailureOwner::ProviderFailure.disposition(FailureContext::new(false, true)),
        FailureDisposition::RecoverProvider,
    );
    assert_eq!(
        FailureOwner::ProviderFailure.disposition(FailureContext::new(false, false)),
        FailureDisposition::Settle,
    );
    assert_eq!(
        FailureOwner::HarnessInfrastructure.disposition(FailureContext::new(false, true)),
        FailureDisposition::RecoverHarness,
    );
    assert_eq!(
        FailureOwner::HarnessInfrastructure.disposition(FailureContext::new(false, false)),
        FailureDisposition::Settle,
    );
    assert_eq!(
        FailureOwner::ExternalEvaluator.disposition(FailureContext::new(false, true)),
        FailureDisposition::Settle,
    );
}
