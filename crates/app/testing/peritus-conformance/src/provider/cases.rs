//! Executable provider-neutral conformance cases.

mod checks;

use super::{ProviderConformanceSubject, ProviderScenario};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct ProviderCase {
    descriptor: CaseDescriptor,
    scenario: ProviderScenario,
}

impl<S: ProviderConformanceSubject> ConformanceCase<S> for ProviderCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(checks::exercise(subject, self.scenario), self.scenario) })
    }
}

/// Returns the complete runtime-neutral model-provider conformance suite.
#[must_use]
pub fn provider_suite<S: ProviderConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.provider"),
            ReportText::literal(
                "C5 provider capability, stream, recovery, accounting, and isolation contract",
            ),
        ),
        vec![
            boxed(
                "adapter-isolation",
                "Configuration, credentials, and transport remain instance-local",
                ProviderScenario::AdapterIsolation,
            ),
            boxed(
                "ambiguous-submission",
                "Maybe-accepted requests are never blindly recreated",
                ProviderScenario::AmbiguousSubmission,
            ),
            boxed(
                "authentication-failure",
                "Authentication failure is typed and non-retryable",
                ProviderScenario::AuthenticationFailure,
            ),
            boxed(
                "cancellation",
                "Cancellation interrupts and joins owned provider work",
                ProviderScenario::Cancellation,
            ),
            boxed(
                "capability-honesty",
                "Advertised features work and unsupported features stop before transport",
                ProviderScenario::CapabilityHonesty,
            ),
            boxed(
                "fragmented-tool-call",
                "Fragmented tool arguments complete only after their close event",
                ProviderScenario::FragmentedToolCall,
            ),
            boxed(
                "incomplete-stream",
                "EOF without a terminal event fails closed",
                ProviderScenario::IncompleteStream,
            ),
            boxed(
                "interruption",
                "Transport interruption remains explicit after partial output",
                ProviderScenario::Interruption,
            ),
            boxed(
                "malformed-payload",
                "Malformed provider payload cannot become success",
                ProviderScenario::MalformedPayload,
            ),
            boxed(
                "ordered-deduplication",
                "Ordering and applicable provider-event deduplication remain deterministic",
                ProviderScenario::OrderedDeduplication,
            ),
            boxed(
                "rate-limit-retry-after",
                "Rate-limit retry honors the bounded provider delay",
                ProviderScenario::RateLimitRetryAfter,
            ),
            boxed(
                "redaction",
                "Sensitive values are absent from every reportable surface",
                ProviderScenario::Redaction,
            ),
            boxed(
                "transient-retry",
                "Transient failure follows one bounded retry plan",
                ProviderScenario::TransientRetry,
            ),
            boxed(
                "usage-accounting",
                "Usage observations are monotonic and exact",
                ProviderScenario::UsageAccounting,
            ),
        ],
    )
}

fn boxed<S: ProviderConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: ProviderScenario,
) -> Box<dyn ConformanceCase<S>> {
    let id = match suffix {
        "adapter-isolation" => "peritus.provider.adapter-isolation",
        "ambiguous-submission" => "peritus.provider.ambiguous-submission",
        "authentication-failure" => "peritus.provider.authentication-failure",
        "cancellation" => "peritus.provider.cancellation",
        "capability-honesty" => "peritus.provider.capability-honesty",
        "fragmented-tool-call" => "peritus.provider.fragmented-tool-call",
        "incomplete-stream" => "peritus.provider.incomplete-stream",
        "interruption" => "peritus.provider.interruption",
        "malformed-payload" => "peritus.provider.malformed-payload",
        "ordered-deduplication" => "peritus.provider.ordered-deduplication",
        "rate-limit-retry-after" => "peritus.provider.rate-limit-retry-after",
        "redaction" => "peritus.provider.redaction",
        "transient-retry" => "peritus.provider.transient-retry",
        _ => "peritus.provider.usage-accounting",
    };
    Box::new(ProviderCase {
        descriptor: CaseDescriptor::new(CaseId::catalog(id), ReportText::literal(summary)),
        scenario,
    })
}

fn result(exact: bool, scenario: ProviderScenario) -> CaseResult {
    let observations =
        vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(scenario))
    }
}

fn assertion(scenario: ProviderScenario) -> AssertionFailure {
    let (code, summary) = match scenario {
        ProviderScenario::CapabilityHonesty => {
            ("PERITUS-PROVIDER-CONFORMANCE-001", "capability profile or probe effects disagreed")
        }
        ProviderScenario::OrderedDeduplication => (
            "PERITUS-PROVIDER-CONFORMANCE-002",
            "event ordering, applicable deduplication, or terminal count drifted",
        ),
        ProviderScenario::FragmentedToolCall => (
            "PERITUS-PROVIDER-CONFORMANCE-003",
            "fragmented tool arguments completed early or with different bytes",
        ),
        ProviderScenario::MalformedPayload => {
            ("PERITUS-PROVIDER-CONFORMANCE-004", "malformed payload was not an explicit failure")
        }
        ProviderScenario::IncompleteStream => (
            "PERITUS-PROVIDER-CONFORMANCE-005",
            "incomplete stream was not failed after partial output",
        ),
        ProviderScenario::Interruption => (
            "PERITUS-PROVIDER-CONFORMANCE-006",
            "transport interruption lost its explicit terminal classification",
        ),
        ProviderScenario::Cancellation => (
            "PERITUS-PROVIDER-CONFORMANCE-007",
            "cancellation lost interruption, ownership, or terminal state",
        ),
        ProviderScenario::AuthenticationFailure => (
            "PERITUS-PROVIDER-CONFORMANCE-008",
            "authentication failure was retried or misclassified",
        ),
        ProviderScenario::RateLimitRetryAfter => (
            "PERITUS-PROVIDER-CONFORMANCE-009",
            "rate-limit retry did not honor the exact bounded delay",
        ),
        ProviderScenario::TransientRetry => (
            "PERITUS-PROVIDER-CONFORMANCE-010",
            "transient retry attempts or bounded delay drifted",
        ),
        ProviderScenario::AmbiguousSubmission => {
            ("PERITUS-PROVIDER-CONFORMANCE-011", "ambiguous submission was retried or concealed")
        }
        ProviderScenario::UsageAccounting => (
            "PERITUS-PROVIDER-CONFORMANCE-012",
            "usage counters regressed or final accounting was inconsistent",
        ),
        ProviderScenario::Redaction => {
            ("PERITUS-PROVIDER-CONFORMANCE-013", "a sensitive canary reached a reportable surface")
        }
        ProviderScenario::AdapterIsolation => (
            "PERITUS-PROVIDER-CONFORMANCE-014",
            "provider configuration or effects crossed adapter instances",
        ),
    };
    AssertionFailure::new(FailureCode::catalog(code), ReportText::literal(summary), None, None)
}
