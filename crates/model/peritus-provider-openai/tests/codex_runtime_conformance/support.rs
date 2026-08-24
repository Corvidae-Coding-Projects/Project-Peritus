//! Portable fake-executable installation and production-provider probes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use peritus_conformance::{ProviderConformanceError, ProviderConformanceFixture, ProviderScenario};
use peritus_model_protocol::{EventEnvelope, ModelEvent, ModelRequest, ProviderProfile};
use peritus_provider_core::{
    CancellationToken, ModelProvider, ProcessLimits, RetryAction, RetryFailure, RetryObservation,
    RetryPlan, RetryPolicy, SubmissionState, wait_for_backoff,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};

mod request;

pub(super) use request::{profile, request};

const HELPER: &str = env!("CARGO_BIN_EXE_peritus-openai-codex-fake");

pub(super) struct Probe {
    pub events: Vec<EventEnvelope>,
    pub trace: Vec<String>,
    pub surfaces: Vec<String>,
    pub directory_removed: bool,
    pub sensitive_inputs: usize,
}

pub(super) struct RecoveryProbe {
    pub first: Vec<EventEnvelope>,
    pub second: Vec<EventEnvelope>,
    pub trace: Vec<String>,
    pub plan: RetryPlan,
    pub directory_removed: bool,
}

impl RecoveryProbe {
    pub fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = fixture.scenario();
        let profile = profile(scenario, 0xD4)?;
        let request = request(&profile, false, None)?;
        let request_bytes = u64::try_from(
            request.canonical_bytes().map_err(|_| ProviderConformanceError::Infrastructure)?.len(),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let helper = FakeExecutable::install(scenario)?;
        let trace_path = helper.trace_path();
        let provider = provider(helper.path(), profile)?;
        let first = run_provider(&provider, request.clone(), scenario, &trace_path)?;
        let trace = read_trace(&trace_path)?;
        let failure = classified_fixture_failure(scenario, &trace)?;
        let policy = RetryPolicy::new(
            2,
            [
                Duration::from_millis(1),
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(10),
            ],
            2 * 1024 * 1024,
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let mut observation = RetryObservation::new(
            1,
            Duration::ZERO,
            request_bytes,
            SubmissionState::Rejected,
            failure,
        );
        if scenario == ProviderScenario::RateLimitRetryAfter {
            observation =
                observation.with_retry_after(Duration::from_millis(fixture.retry_after_millis()));
        }
        let plan =
            policy.plan(observation).map_err(|_| ProviderConformanceError::Infrastructure)?;
        if plan.action() != RetryAction::RetryFresh {
            return Err(ProviderConformanceError::Infrastructure);
        }
        run_backoff(plan)?;
        let second = run_provider(&provider, request, scenario, &trace_path)?;
        drop(provider);
        let trace = read_trace(&trace_path)?;
        let directory_removed = helper.close();
        Ok(Self { first, second, trace, plan, directory_removed })
    }
}

impl Probe {
    pub fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = fixture.scenario();
        let profile = profile(scenario, 0xD2)?;
        let canary = (scenario == ProviderScenario::Redaction).then_some(fixture.canary());
        let with_tools = matches!(
            scenario,
            ProviderScenario::CapabilityHonesty
                | ProviderScenario::FragmentedToolCall
                | ProviderScenario::Redaction
        );
        let request = request(&profile, with_tools, canary)?;
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
    ) -> Result<Self, ProviderConformanceError> {
        let helper = FakeExecutable::install(scenario)?;
        let trace_path = helper.trace_path();
        let provider = provider(helper.path(), profile)?;
        let mut surfaces = vec![format!("{request:?}"), format!("{provider:?}")];
        let events = run_provider(&provider, request, scenario, &trace_path)?;
        surfaces.extend(events.iter().map(|event| format!("{event:?}")));
        drop(provider);
        let trace = read_trace(&trace_path)?;
        surfaces.extend(trace.iter().cloned());
        let directory_removed = helper.close();
        Ok(Self { events, trace, surfaces, directory_removed, sensitive_inputs: 0 })
    }

    pub fn auth_requests(&self) -> usize {
        count(&self.trace, "auth")
    }

    pub fn turn_requests(&self) -> usize {
        count(&self.trace, "turn")
    }

    pub fn completed(&self) -> bool {
        matches!(self.events.last().map(EventEnvelope::event), Some(ModelEvent::ResponseCompleted))
    }
}

pub(super) struct ForeignProbe {
    helper: FakeExecutable,
    _provider: CodexRuntimeProvider,
}

impl ForeignProbe {
    pub fn untouched() -> Result<Self, ProviderConformanceError> {
        let helper = FakeExecutable::install(ProviderScenario::AdapterIsolation)?;
        let profile = profile(ProviderScenario::AdapterIsolation, 0xD3)?;
        let provider = provider(helper.path(), profile)?;
        Ok(Self { helper, _provider: provider })
    }

    pub fn requests(&self) -> Result<usize, ProviderConformanceError> {
        Ok(read_trace(&self.helper.trace_path())?.len())
    }
}

fn provider(
    executable_path: &Path,
    profile: ProviderProfile,
) -> Result<CodexRuntimeProvider, ProviderConformanceError> {
    let executable = CodexExecutable::pin(executable_path)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let limits =
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(10))
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let config = CodexRuntimeConfig::new(executable, profile, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    Ok(CodexRuntimeProvider::new(config))
}

fn run_provider(
    provider: &CodexRuntimeProvider,
    request: ModelRequest,
    scenario: ProviderScenario,
    trace_path: &Path,
) -> Result<Vec<EventEnvelope>, ProviderConformanceError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| ProviderConformanceError::Infrastructure)?;
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
                    let mut stream = provider
                        .start(request, cancellation)
                        .await
                        .map_err(|_| ProviderConformanceError::Infrastructure)?;
                    let mut events = Vec::new();
                    while let Some(event) =
                        stream.pull().await.map_err(|_| ProviderConformanceError::Infrastructure)?
                    {
                        let terminal = is_terminal(event.event());
                        events.push(event);
                        if terminal {
                            break;
                        }
                    }
                    if let Some(watcher) = watcher
                        && !watcher.await.map_err(|_| ProviderConformanceError::Infrastructure)?
                    {
                        return Err(ProviderConformanceError::Infrastructure);
                    }
                    Ok(events)
                })
            })
            .join()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
    })
}

fn run_backoff(plan: RetryPlan) -> Result<(), ProviderConformanceError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| ProviderConformanceError::Infrastructure)?;
                runtime
                    .block_on(wait_for_backoff(plan, &CancellationToken::new()))
                    .map_err(|_| ProviderConformanceError::Infrastructure)
            })
            .join()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
    })
}

fn classified_fixture_failure(
    scenario: ProviderScenario,
    trace: &[String],
) -> Result<RetryFailure, ProviderConformanceError> {
    match scenario {
        ProviderScenario::RateLimitRetryAfter
            if trace.iter().any(|entry| entry == "failure-rate-limited-250") =>
        {
            Ok(RetryFailure::RateLimited)
        }
        ProviderScenario::TransientRetry
            if trace.iter().any(|entry| entry == "failure-transient-0") =>
        {
            Ok(RetryFailure::Server)
        }
        _ => Err(ProviderConformanceError::Infrastructure),
    }
}

fn read_trace(path: &Path) -> Result<Vec<String>, ProviderConformanceError> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(value.lines().map(str::to_owned).collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err(ProviderConformanceError::Infrastructure),
    }
}

fn count(trace: &[String], expected: &str) -> usize {
    trace.iter().filter(|entry| entry.as_str() == expected).count()
}

struct FakeExecutable {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl FakeExecutable {
    fn install(scenario: ProviderScenario) -> Result<Self, ProviderConformanceError> {
        let directory =
            tempfile::tempdir().map_err(|_| ProviderConformanceError::Infrastructure)?;
        let source = Path::new(HELPER);
        let extension = source.extension().and_then(std::ffi::OsStr::to_str);
        let name = extension.map_or_else(
            || format!("codex-{}", slug(scenario)),
            |extension| format!("codex-{}.{}", slug(scenario), extension),
        );
        let path = directory.path().join(name);
        std::fs::copy(source, &path).map_err(|_| ProviderConformanceError::Infrastructure)?;
        Ok(Self { directory, path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn trace_path(&self) -> PathBuf {
        self.directory.path().join("trace")
    }

    fn close(self) -> bool {
        self.directory.close().is_ok()
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
