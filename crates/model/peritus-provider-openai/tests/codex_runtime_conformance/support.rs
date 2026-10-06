//! Portable fake-executable installation and production-provider probes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use peritus_conformance::{ProviderConformanceFixture, ProviderScenario};
use peritus_model_protocol::{EventEnvelope, ModelEvent, ModelRequest, ProviderProfile};
use peritus_provider_core::{
    CancellationToken, ModelProvider, ProcessLimits, RetryPlan, wait_for_backoff,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};

#[cfg(test)]
mod fixtures;
mod foreign;
#[cfg(test)]
mod receipt_tests;
mod recovery;
mod request;
#[cfg(test)]
mod tests;

use super::diagnostics::{ProbeError, event_evidence, scoped_evidence};

pub(super) use foreign::ForeignProbe;
pub(super) use recovery::RecoveryProbe;

pub(super) use request::{profile, request};

const HELPER: &str = env!("CARGO_BIN_EXE_peritus-openai-codex-fake");

pub(super) struct Probe {
    pub events: Vec<EventEnvelope>,
    pub trace: Vec<String>,
    pub surfaces: Vec<String>,
    pub directory_removed: bool,
    pub sensitive_inputs: usize,
}

impl Probe {
    pub fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProbeError> {
        let scenario = fixture.scenario();
        let profile = profile(scenario, 0xD2).map_err(|_| ProbeError::stage("probe.profile"))?;
        let canary = (scenario == ProviderScenario::Redaction).then_some(fixture.canary());
        let with_tools = matches!(
            scenario,
            ProviderScenario::CapabilityHonesty
                | ProviderScenario::FragmentedToolCall
                | ProviderScenario::Redaction
        );
        let request = request(&profile, with_tools, canary)
            .map_err(|_| ProbeError::stage("probe.request"))?;
        let sensitive_inputs =
            canary.map_or(0, |value| super::redaction::request_canary_count(&request, value));
        let mut probe = Self::run_request(scenario, profile, request)?;
        probe.sensitive_inputs = sensitive_inputs;
        Ok(probe)
    }

    pub fn run_request(
        scenario: ProviderScenario,
        profile: ProviderProfile,
        request: ModelRequest,
    ) -> Result<Self, ProbeError> {
        let helper = FakeExecutable::install(scenario)?;
        Self::run_installed(scenario, profile, request, helper)
    }

    fn run_installed(
        scenario: ProviderScenario,
        profile: ProviderProfile,
        request: ModelRequest,
        helper: FakeExecutable,
    ) -> Result<Self, ProbeError> {
        let trace_path = helper.trace_path();
        let result = (|| {
            let provider = provider(helper.path(), profile)?;
            let mut surfaces = vec![format!("{request:?}"), format!("{provider:?}")];
            let events = run_provider(&provider, request, scenario, &trace_path)?;
            surfaces.extend(events.iter().map(|event| format!("{event:?}")));
            Ok((events, surfaces))
        })();
        let evidence =
            result.as_ref().map_or_else(|_| Vec::new(), |(events, _)| event_evidence(events));
        let ((events, mut surfaces), trace) = helper.finish(result, evidence)?;
        surfaces.extend(trace.iter().cloned());
        Ok(Self { events, trace, surfaces, directory_removed: true, sensitive_inputs: 0 })
    }

    pub fn auth_requests(&self) -> usize {
        count(&self.trace, "auth")
    }

    pub fn turn_requests(&self) -> usize {
        count(&self.trace, "turn")
    }

    pub fn evidence(&self, scope: &'static str) -> Vec<peritus_conformance::Observation> {
        scoped_evidence(
            scope,
            ProbeError::observed(
                "probe.observed",
                &self.events,
                &self.trace,
                self.directory_removed,
            )
            .into_observations(),
        )
    }

    pub fn completed(&self) -> bool {
        matches!(self.events.last().map(EventEnvelope::event), Some(ModelEvent::ResponseCompleted))
    }
}

fn provider(
    executable_path: &Path,
    profile: ProviderProfile,
) -> Result<CodexRuntimeProvider, ProbeError> {
    let executable = CodexExecutable::pin(executable_path)
        .map_err(|error| ProbeError::provider("executable.pin", &error))?;
    let limits =
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(10))
            .map_err(|error| ProbeError::provider("process.limits", &error))?;
    let config = CodexRuntimeConfig::new(executable, profile, limits)
        .map_err(|error| ProbeError::provider("provider.configure", &error))?;
    Ok(CodexRuntimeProvider::new(config))
}

fn run_provider(
    provider: &CodexRuntimeProvider,
    request: ModelRequest,
    scenario: ProviderScenario,
    trace_path: &Path,
) -> Result<Vec<EventEnvelope>, ProbeError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| ProbeError::io("runtime.build", &error))?;
                runtime.block_on(async {
                    let cancellation = CancellationToken::new();
                    let watcher = if scenario == ProviderScenario::Cancellation {
                        let control = cancellation.clone();
                        let path = trace_path.to_path_buf();
                        Some(tokio::spawn(async move {
                            for _ in 0..500 {
                                if std::fs::read_to_string(&path)
                                    .is_ok_and(|value| value.lines().any(|line| line == "spin"))
                                {
                                    return control.cancel();
                                }
                                tokio::time::sleep(Duration::from_millis(20)).await;
                            }
                            false
                        }))
                    } else {
                        None
                    };
                    let mut stream =
                        provider.start(request, cancellation).await.map_err(|error| {
                            ProbeError::provider("provider.start", &error).with_events(&[])
                        })?;
                    let mut events = Vec::new();
                    while let Some(event) = stream.pull().await.map_err(|error| {
                        ProbeError::provider("stream.pull", &error).with_events(&events)
                    })? {
                        let terminal = is_terminal(event.event());
                        events.push(event);
                        if terminal {
                            break;
                        }
                    }
                    if let Some(watcher) = watcher
                        && !watcher
                            .await
                            .map_err(|_| ProbeError::stage("watcher.join").with_events(&events))?
                    {
                        return Err(ProbeError::stage("watcher.cancel").with_events(&events));
                    }
                    Ok(events)
                })
            })
            .join()
            .map_err(|_| ProbeError::stage("worker.join"))?
    })
}

fn run_backoff(plan: RetryPlan) -> Result<(), ProbeError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| ProbeError::io("backoff.runtime", &error))?;
                runtime
                    .block_on(wait_for_backoff(plan, &CancellationToken::new()))
                    .map_err(|error| ProbeError::provider("backoff.wait", &error))
            })
            .join()
            .map_err(|_| ProbeError::stage("backoff.join"))?
    })
}

fn read_trace(path: &Path) -> Result<Vec<String>, std::io::Error> {
    std::fs::read_to_string(path).map(|value| value.lines().map(str::to_owned).collect())
}

fn count(trace: &[String], expected: &str) -> usize {
    trace.iter().filter(|entry| entry.as_str() == expected).count()
}

struct FakeExecutable {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl FakeExecutable {
    fn install(scenario: ProviderScenario) -> Result<Self, ProbeError> {
        let source = Path::new(HELPER);
        let parent = source.parent().ok_or_else(|| ProbeError::stage("fixture.source-parent"))?;
        // Link the immutable Cargo artifact on its own filesystem. Copying executable bytes
        // during concurrent forks can leave an inherited writable descriptor alive after the
        // copying thread closes it, making the next exec fail with ExecutableFileBusy.
        let directory = tempfile::Builder::new()
            .prefix("peritus-codex-fixture-")
            .tempdir_in(parent)
            .map_err(|error| ProbeError::io("fixture.directory", &error))?;
        let extension = source.extension().and_then(std::ffi::OsStr::to_str);
        let name = extension.map_or_else(
            || format!("codex-{}", slug(scenario)),
            |extension| format!("codex-{}.{}", slug(scenario), extension),
        );
        let path = directory.path().join(name);
        if let Err(error) = std::fs::hard_link(source, &path) {
            let mut failure = ProbeError::io("fixture.link", &error);
            failure.add_cleanup(&directory.close());
            return Err(failure);
        }
        Ok(Self { directory, path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn trace_path(&self) -> PathBuf {
        self.directory.path().join("trace")
    }

    fn finish<T>(
        self,
        result: Result<T, ProbeError>,
        evidence: Vec<peritus_conformance::Observation>,
    ) -> Result<(T, Vec<String>), ProbeError> {
        let trace = read_trace(&self.trace_path());
        let cleanup = self.directory.close();
        let mut failure = match result {
            Ok(value) => match (&trace, &cleanup) {
                (Ok(_), Ok(())) => return Ok((value, trace.expect("successful trace read"))),
                (Err(error), _) => ProbeError::io("trace.read", error).with_evidence(evidence),
                (_, Err(error)) => ProbeError::io("fixture.cleanup", error).with_evidence(evidence),
            },
            Err(failure) => failure,
        };
        failure.add_trace(trace.as_deref());
        failure.add_cleanup(&cleanup);
        Err(failure)
    }
}

const fn slug(scenario: ProviderScenario) -> &'static str {
    match scenario {
        ProviderScenario::CapabilityHonesty => "capability",
        ProviderScenario::OrderedDeduplication => "ordered",
        ProviderScenario::FragmentedToolCall => "fragmented-tool",
        ProviderScenario::MalformedPayload => "malformed",
        ProviderScenario::IncompleteStream => "incomplete",
        ProviderScenario::Interruption => "interruption",
        ProviderScenario::Cancellation => "cancel",
        ProviderScenario::AuthenticationFailure => "authentication",
        ProviderScenario::RateLimitRetryAfter => "rate-limit",
        ProviderScenario::TransientRetry => "transient",
        ProviderScenario::AmbiguousSubmission => "ambiguous",
        ProviderScenario::UsageAccounting => "usage",
        ProviderScenario::Redaction => "redaction",
        ProviderScenario::AdapterIsolation => "isolation",
    }
}

pub(super) const fn is_terminal(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled
    )
}
