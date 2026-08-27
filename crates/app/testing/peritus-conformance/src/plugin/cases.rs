//! Executable G3 plugin conformance cases.

use super::{
    PluginConformanceFixture, PluginConformanceSubject, PluginDisposition, PluginScenario,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct PluginCase {
    descriptor: CaseDescriptor,
    scenario: PluginScenario,
}

impl<S: PluginConformanceSubject> ConformanceCase<S> for PluginCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            let exact = subject.exercise(&PluginConformanceFixture::new(self.scenario)).is_ok_and(
                |observed| match self.scenario {
                    PluginScenario::CanonicalManifest => observed.canonical_identity(),
                    PluginScenario::TrustRequired => {
                        observed.disposition() == PluginDisposition::Rejected
                            && observed.trust_checked()
                            && observed.plugin_effects() == 0
                    }
                    PluginScenario::AuthorityDenied => {
                        observed.disposition() == PluginDisposition::Rejected
                            && observed.authority_checked()
                            && observed.plugin_effects() == 0
                    }
                    PluginScenario::Lifecycle => {
                        observed.disposition() == PluginDisposition::Succeeded
                            && observed.trust_checked()
                            && observed.authority_checked()
                            && observed.plugin_effects() == 1
                            && observed.runtime_terminated()
                            && observed.runtime_joined()
                    }
                    PluginScenario::Quota => {
                        observed.disposition() == PluginDisposition::Failed
                            && observed.output_bounded()
                            && observed.truthful_failure()
                            && observed.runtime_terminated()
                    }
                    PluginScenario::Cancellation => {
                        observed.disposition() == PluginDisposition::Cancelled
                            && observed.runtime_terminated()
                            && observed.runtime_joined()
                    }
                    PluginScenario::CrashIsolation => {
                        observed.disposition() == PluginDisposition::Failed
                            && observed.host_alive()
                            && observed.truthful_failure()
                    }
                },
            );
            result(exact, self.scenario)
        })
    }
}

/// Returns the complete runtime-neutral G3 plugin conformance suite.
#[must_use]
pub fn plugin_suite<S: PluginConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.plugin"),
            ReportText::literal(
                "G3 canonical trust, authority, isolation, quota, and lifecycle contract",
            ),
        ),
        vec![
            boxed(
                "authority-no-effect",
                "Authority denial reaches no plugin effect",
                PluginScenario::AuthorityDenied,
            ),
            boxed(
                "cancellation",
                "Cancellation terminates and joins isolated work",
                PluginScenario::Cancellation,
            ),
            boxed(
                "canonical-manifest",
                "Manifest and artifact identity is canonical",
                PluginScenario::CanonicalManifest,
            ),
            boxed(
                "crash-isolation",
                "Plugin failure cannot terminate the host",
                PluginScenario::CrashIsolation,
            ),
            boxed(
                "lifecycle",
                "Trusted plugin lifecycle is negotiated and owned",
                PluginScenario::Lifecycle,
            ),
            boxed("quota", "Host ceilings bound plugin results", PluginScenario::Quota),
            boxed(
                "trust-required",
                "Unknown plugin bytes cannot start",
                PluginScenario::TrustRequired,
            ),
        ],
    )
}

fn boxed<S: PluginConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: PluginScenario,
) -> Box<dyn ConformanceCase<S>> {
    let id = match suffix {
        "authority-no-effect" => "peritus.plugin.authority-no-effect",
        "cancellation" => "peritus.plugin.cancellation",
        "canonical-manifest" => "peritus.plugin.canonical-manifest",
        "crash-isolation" => "peritus.plugin.crash-isolation",
        "lifecycle" => "peritus.plugin.lifecycle",
        "quota" => "peritus.plugin.quota",
        _ => "peritus.plugin.trust-required",
    };
    Box::new(PluginCase {
        descriptor: CaseDescriptor::new(CaseId::catalog(id), ReportText::literal(summary)),
        scenario,
    })
}

fn result(exact: bool, scenario: PluginScenario) -> CaseResult {
    let observations =
        vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(scenario))
    }
}

fn assertion(scenario: PluginScenario) -> AssertionFailure {
    let (code, summary) = match scenario {
        PluginScenario::CanonicalManifest => ("001", "plugin identity was not canonical"),
        PluginScenario::TrustRequired => ("002", "untrusted plugin reached execution"),
        PluginScenario::AuthorityDenied => ("003", "authority denial reached a plugin effect"),
        PluginScenario::Lifecycle => ("004", "plugin lifecycle was not fully owned"),
        PluginScenario::Quota => ("005", "host quota did not bound the plugin result"),
        PluginScenario::Cancellation => ("006", "cancelled plugin work remained owned"),
        PluginScenario::CrashIsolation => ("007", "plugin failure escaped isolation"),
    };
    AssertionFailure::new(
        FailureCode::catalog(match code {
            "001" => "PERITUS-PLUGIN-CONFORMANCE-001",
            "002" => "PERITUS-PLUGIN-CONFORMANCE-002",
            "003" => "PERITUS-PLUGIN-CONFORMANCE-003",
            "004" => "PERITUS-PLUGIN-CONFORMANCE-004",
            "005" => "PERITUS-PLUGIN-CONFORMANCE-005",
            "006" => "PERITUS-PLUGIN-CONFORMANCE-006",
            _ => "PERITUS-PLUGIN-CONFORMANCE-007",
        }),
        ReportText::literal(summary),
        None,
        None,
    )
}
