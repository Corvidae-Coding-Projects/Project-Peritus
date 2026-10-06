//! Owned recovery attempts keep every observed terminal through subsequent failures.

use std::path::Path;
use std::time::Duration;

use peritus_conformance::{Observation, ProviderConformanceFixture, ProviderScenario};
use peritus_model_protocol::{EventEnvelope, ModelRequest, ProviderProfile};
use peritus_provider_core::{
    RetryAction, RetryFailure, RetryObservation, RetryPlan, RetryPolicy, SubmissionState,
};
use peritus_provider_openai::CodexRuntimeProvider;

use super::super::diagnostics::{ProbeError, event_evidence, scoped_evidence};
use super::{FakeExecutable, profile, provider, read_trace, request, run_backoff, run_provider};

pub(in super::super) struct RecoveryProbe {
    pub first: Vec<EventEnvelope>,
    pub second: Vec<EventEnvelope>,
    pub trace: Vec<String>,
    pub plan: RetryPlan,
    pub directory_removed: bool,
}

impl RecoveryProbe {
    pub fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProbeError> {
        let scenario = fixture.scenario();
        let profile = profile(scenario, 0xD4).map_err(|_| ProbeError::stage("recovery.profile"))?;
        let request =
            request(&profile, false, None).map_err(|_| ProbeError::stage("recovery.request"))?;
        let request_bytes = u64::try_from(
            request.canonical_bytes().map_err(|_| ProbeError::stage("request.canonical"))?.len(),
        )
        .map_err(|_| ProbeError::stage("request.length"))?;
        Self::run_installed(
            fixture,
            request,
            request_bytes,
            profile,
            FakeExecutable::install(scenario)?,
        )
    }

    pub(super) fn run_installed(
        fixture: &ProviderConformanceFixture,
        request: ModelRequest,
        request_bytes: u64,
        profile: ProviderProfile,
        helper: FakeExecutable,
    ) -> Result<Self, ProbeError> {
        let trace_path = helper.trace_path();
        let result =
            attempts(fixture, request, request_bytes, &helper, profile, &trace_path, run_provider);
        let evidence = result
            .as_ref()
            .map_or_else(|_| Vec::new(), |(first, second, _)| attempt_evidence(first, second));
        let ((first, second, plan), trace) = helper.finish(result, evidence)?;
        Ok(Self { first, second, trace, plan, directory_removed: true })
    }

    pub fn error(&self, stage: &'static str) -> ProbeError {
        let mut failure =
            ProbeError::stage(stage).with_evidence(attempt_evidence(&self.first, &self.second));
        failure.add_trace(Ok(&self.trace));
        failure.add_directory_removed(self.directory_removed);
        failure
    }
}

pub(super) fn attempt_evidence(
    first: &[EventEnvelope],
    second: &[EventEnvelope],
) -> Vec<Observation> {
    let mut evidence = scoped_evidence("recovery.first", event_evidence(first));
    evidence.extend(scoped_evidence("recovery.second", event_evidence(second)));
    evidence
}

pub(super) fn attempts(
    fixture: &ProviderConformanceFixture,
    request: ModelRequest,
    request_bytes: u64,
    helper: &FakeExecutable,
    profile: ProviderProfile,
    trace_path: &Path,
    mut run: impl FnMut(
        &CodexRuntimeProvider,
        ModelRequest,
        ProviderScenario,
        &Path,
    ) -> Result<Vec<EventEnvelope>, ProbeError>,
) -> Result<(Vec<EventEnvelope>, Vec<EventEnvelope>, RetryPlan), ProbeError> {
    let scenario = fixture.scenario();
    let provider = provider(helper.path(), profile)?;
    let first = run(&provider, request.clone(), scenario, trace_path)
        .map_err(|failure| failure.with_scope("recovery.first"))?;
    let result = (|| {
        let trace =
            read_trace(trace_path).map_err(|error| ProbeError::io("recovery.trace", &error))?;
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
        .map_err(|error| ProbeError::provider("recovery.policy", &error))?;
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
        let plan = policy
            .plan(observation)
            .map_err(|error| ProbeError::provider("recovery.plan", &error))?;
        if plan.action() != RetryAction::RetryFresh {
            return Err(ProbeError::stage("recovery.action"));
        }
        run_backoff(plan)?;
        let second = run(&provider, request, scenario, trace_path)
            .map_err(|failure| failure.with_scope("recovery.second"))?;
        Ok((second, plan))
    })()
    .map_err(|failure: ProbeError| {
        failure.with_evidence(scoped_evidence("recovery.first", event_evidence(&first)))
    });
    let (second, plan) = result?;
    Ok((first, second, plan))
}

fn classified_fixture_failure(
    scenario: ProviderScenario,
    trace: &[String],
) -> Result<RetryFailure, ProbeError> {
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
        _ => Err(ProbeError::stage("recovery.fixture-classification")),
    }
}
