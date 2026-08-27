//! Executable catalog for the runtime-neutral G0 daemon contract.

mod checks;

use super::{DAEMON_SCENARIOS, DaemonConformanceFixture, DaemonConformanceSubject, DaemonScenario};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct DaemonCase {
    descriptor: CaseDescriptor,
    scenario: DaemonScenario,
}

impl<S: DaemonConformanceSubject> ConformanceCase<S> for DaemonCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns all runtime-neutral G0 daemon black-box conformance cases.
#[must_use]
pub fn daemon_suite<S: DaemonConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.daemon"),
            ReportText::literal(
                "G0 local protocol, authority, service, recovery, bounds, and lifecycle contract",
            ),
        ),
        DAEMON_SCENARIOS.iter().copied().map(scenario_case).collect(),
    )
}

/// Returns one selected G0 scenario under the same descriptor and assertion contract.
///
/// This supports incremental production-adapter qualification without treating unavailable
/// scenarios as success. Only the complete nonempty [`daemon_suite`] proves full G0 conformance.
#[must_use]
pub fn daemon_scenario_suite<S: DaemonConformanceSubject + 'static>(
    scenario: DaemonScenario,
) -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.daemon.slice"),
            ReportText::literal("One selected G0 daemon black-box contract case"),
        ),
        vec![scenario_case(scenario)],
    )
}

fn scenario_case<S: DaemonConformanceSubject + 'static>(
    scenario: DaemonScenario,
) -> Box<dyn ConformanceCase<S>> {
    let (suffix, summary) = metadata(scenario);
    boxed(suffix, summary, scenario)
}

const fn metadata(scenario: DaemonScenario) -> (&'static str, &'static str) {
    use DaemonScenario as D;
    match scenario {
        D::ArtifactCorruption => {
            ("artifact-corruption", "Corrupt artifacts publish no partial authority")
        }
        D::ArtifactDownload => (
            "artifact-download",
            "Artifact download conserves identity, bytes, offsets, and digest",
        ),
        D::ArtifactUpload => {
            ("artifact-upload", "Artifact upload publishes only after exact finalization")
        }
        D::Bounds => {
            ("bounds", "Oversized work is rejected before allocation and retention remains bounded")
        }
        D::ConflictingCommand => {
            ("command-conflict", "A reused key with a different digest conflicts without effect")
        }
        D::IndeterminateCommand => (
            "command-indeterminate",
            "An ambiguous command retains and reconciles its original identity",
        ),
        D::NewCommand => ("command-new", "A new command reports its exact committed range"),
        D::ReplayCommand => ("command-replay", "An exact retry replays without append or effect"),
        D::ContextMismatch => ("context-mismatch", "Negotiated protocol context mismatch is inert"),
        D::ForcedRestart => {
            ("forced-restart", "Forced restart reconciles durable work without duplication")
        }
        D::MalformedFrame => {
            ("malformed-frame", "Malformed framing is rejected before allocation or dispatch")
        }
        D::NonAuthority => {
            ("non-authority", "Report-only surfaces cannot mutate or imply acceptance")
        }
        D::OutboxCrash => {
            ("outbox-crash", "Effect-before-ack recovery reconciles and acknowledges once")
        }
        D::PeerActorMismatch => {
            ("peer-actor-mismatch", "Live peer and durable actor mismatch is inert")
        }
        D::PromptFreshness => {
            ("prompt-freshness", "Prompt settlement requires current complete authority")
        }
        D::PtyOrdering => {
            ("pty-ordering", "Combined PTY bytes, offsets, bounds, and exit remain exact")
        }
        D::ReadOnlyAdmission => {
            ("read-only-admission", "Read-only readiness admits observation but no mutation")
        }
        D::StaleRevision => {
            ("revision-stale", "Stale command authority is rejected before append or effect")
        }
        D::CompatibleSession => (
            "session-compatible",
            "A compatible authenticated client establishes its durable session",
        ),
        D::IncompatibleSession => {
            ("session-incompatible", "An incompatible hello establishes no session")
        }
        D::GracefulShutdown => {
            ("shutdown-graceful", "Graceful shutdown reports clean only after complete drain")
        }
        D::StartupFailure => {
            ("startup-failure", "Diagnostic-safe startup failure remains read-only and inert")
        }
        D::SubscriptionAcknowledgement => {
            ("subscription-ack", "Acknowledgement releases only a delivered prefix")
        }
        D::SubscriptionBackpressure => {
            ("subscription-backpressure", "A slow subscriber stays inside its in-flight bound")
        }
        D::SubscriptionGap => {
            ("subscription-gap", "A retention gap explicitly requires a snapshot")
        }
        D::SubscriptionRedelivery => (
            "subscription-redelivery",
            "Redelivery retains event identity and changes attempt identity",
        ),
        D::SubscriptionResume => {
            ("subscription-resume", "Resume starts strictly after the supplied source cursor")
        }
        D::SecondInstance => ("second-instance", "A second daemon leaves the live owner untouched"),
    }
}

fn boxed<S: DaemonConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: DaemonScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(DaemonCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.daemon.{suffix}")).expect("static daemon case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: DaemonConformanceSubject>(subject: &mut S, scenario: DaemonScenario) -> CaseResult {
    let fixture = DaemonConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, false);
    };
    let exact = checks::matches_contract(&fixture, &observed);
    if exact { CaseResult::passed(observations(true)) } else { failed(scenario, false) }
}

fn failed(scenario: DaemonScenario, exact: bool) -> CaseResult {
    CaseResult::failed(observations(exact), assertion(scenario))
}

fn observations(exact: bool) -> Vec<Observation> {
    vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))]
}

fn assertion(scenario: DaemonScenario) -> AssertionFailure {
    let number = match scenario {
        DaemonScenario::CompatibleSession => "001",
        DaemonScenario::IncompatibleSession => "002",
        DaemonScenario::PeerActorMismatch => "003",
        DaemonScenario::ContextMismatch => "004",
        DaemonScenario::NewCommand => "005",
        DaemonScenario::ReplayCommand => "006",
        DaemonScenario::ConflictingCommand => "007",
        DaemonScenario::IndeterminateCommand => "008",
        DaemonScenario::StaleRevision => "009",
        DaemonScenario::SubscriptionResume => "010",
        DaemonScenario::SubscriptionRedelivery => "011",
        DaemonScenario::SubscriptionAcknowledgement => "012",
        DaemonScenario::SubscriptionGap => "013",
        DaemonScenario::SubscriptionBackpressure => "014",
        DaemonScenario::ArtifactDownload => "015",
        DaemonScenario::ArtifactUpload => "016",
        DaemonScenario::ArtifactCorruption => "017",
        DaemonScenario::PromptFreshness => "018",
        DaemonScenario::PtyOrdering => "019",
        DaemonScenario::ReadOnlyAdmission => "020",
        DaemonScenario::SecondInstance => "021",
        DaemonScenario::StartupFailure => "022",
        DaemonScenario::OutboxCrash => "023",
        DaemonScenario::GracefulShutdown => "024",
        DaemonScenario::ForcedRestart => "025",
        DaemonScenario::Bounds => "026",
        DaemonScenario::MalformedFrame => "027",
        DaemonScenario::NonAuthority => "028",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-DAEMON-CONFORMANCE-{number}"))
            .expect("static daemon failure code"),
        ReportText::literal("direct observations violated the selected G0 daemon contract"),
        None,
        None,
    )
}
