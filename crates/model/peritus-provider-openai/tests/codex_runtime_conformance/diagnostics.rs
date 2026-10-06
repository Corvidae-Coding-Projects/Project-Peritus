//! Per-exercise redacted failure evidence, owned by the failed probe.

use peritus_conformance::{Observation, ObservationId, ObservationValue, ReportText};
use peritus_model_protocol::{EventEnvelope, ModelEvent};
use peritus_provider_core::ProviderCoreError;

#[derive(Debug)]
pub(super) struct ProbeError {
    observations: Vec<Observation>,
}

impl ProbeError {
    pub fn stage(stage: &'static str) -> Self {
        Self { observations: vec![text("probe.stage", stage)] }
    }

    pub fn io(stage: &'static str, error: &std::io::Error) -> Self {
        let mut result = Self::stage(stage);
        result.add_io("probe.io-kind", "probe.os-code", error);
        result
    }

    pub fn provider(stage: &'static str, error: &ProviderCoreError) -> Self {
        let mut result = Self::stage(stage);
        result.observations.extend([
            text("probe.provider-code", error.code()),
            text("probe.provider-operation", error.operation()),
            text("probe.provider-detail", error.detail()),
        ]);
        if let Some(kind) = error.io_kind() {
            result.observations.push(text("probe.io-kind", format!("{kind:?}")));
        }
        if let Some(code) = error.raw_os_error() {
            result
                .observations
                .push(observation("probe.os-code", ObservationValue::Signed(i64::from(code))));
        }
        result
    }

    pub fn observed(
        stage: &'static str,
        events: &[EventEnvelope],
        trace: &[String],
        directory_removed: bool,
    ) -> Self {
        let mut result = Self::stage(stage);
        result.observations.extend(event_evidence(events));
        result.add_trace(Ok(trace));
        result.add_directory_removed(directory_removed);
        result
    }

    pub fn with_events(mut self, events: &[EventEnvelope]) -> Self {
        self.observations.extend(event_evidence(events));
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<Observation>) -> Self {
        self.observations.extend(evidence);
        self
    }

    pub fn with_scope(mut self, scope: &'static str) -> Self {
        self.observations.push(text("probe.scope", scope));
        self
    }

    pub fn add_trace(&mut self, trace: Result<&[String], &std::io::Error>) {
        self.observations.push(boolean("probe.trace-readable", trace.is_ok()));
        match trace {
            Ok(trace) => self.observations.extend([
                unsigned(
                    "probe.auth-requests",
                    trace.iter().filter(|line| *line == "auth").count(),
                ),
                unsigned(
                    "probe.turn-requests",
                    trace.iter().filter(|line| *line == "turn").count(),
                ),
            ]),
            Err(error) => self.add_io("probe.trace-io-kind", "probe.trace-os-code", error),
        }
    }

    pub fn add_cleanup(&mut self, cleanup: &Result<(), std::io::Error>) {
        self.add_directory_removed(cleanup.is_ok());
        if let Err(error) = cleanup {
            self.add_io("probe.cleanup-io-kind", "probe.cleanup-os-code", error);
        }
    }

    pub fn add_directory_removed(&mut self, removed: bool) {
        self.observations.push(boolean("probe.directory-removed", removed));
    }

    pub fn into_observations(self) -> Vec<Observation> {
        self.observations
    }

    fn add_io(&mut self, kind: &str, code: &str, error: &std::io::Error) {
        // Error::Display can include sensitive paths or payloads. Only OS categories and codes
        // cross the conformance report boundary; the original error remains in its I/O owner.
        self.observations.push(text(kind, format!("{:?}", error.kind())));
        if let Some(raw) = error.raw_os_error() {
            self.observations.push(observation(code, ObservationValue::Signed(i64::from(raw))));
        }
    }
}

pub(super) fn scoped_evidence(scope: &'static str, evidence: Vec<Observation>) -> Vec<Observation> {
    evidence
        .into_iter()
        .map(|fact| observation(&format!("{scope}.{}", fact.id()), fact.value().clone()))
        .collect()
}

pub(super) fn event_evidence(events: &[EventEnvelope]) -> Vec<Observation> {
    let terminal = events.last().map_or("absent", |event| match event.event() {
        ModelEvent::ResponseCompleted => "completed",
        ModelEvent::ResponseFailed(_) => "failed",
        ModelEvent::ResponseCancelled => "cancelled",
        _ => "absent",
    });
    let category = events.iter().rev().find_map(|event| match event.event() {
        ModelEvent::ResponseFailed(failure) => Some(format!("{:?}", failure.category())),
        _ => None,
    });
    vec![
        unsigned("probe.events", events.len()),
        text("probe.terminal", terminal),
        text("probe.failure-category", category.as_deref().unwrap_or("absent")),
    ]
}

fn observation(id: &str, value: ObservationValue) -> Observation {
    Observation::new(ObservationId::new(id).expect("fixed observation ID"), value)
}

fn text(id: &str, value: impl Into<String>) -> Observation {
    observation(id, ObservationValue::Text(ReportText::new(value).expect("redacted probe fact")))
}

pub(super) fn boolean(id: &str, value: bool) -> Observation {
    observation(id, ObservationValue::Boolean(value))
}

pub(super) fn unsigned(id: &str, value: usize) -> Observation {
    observation(
        id,
        ObservationValue::Unsigned(u64::try_from(value).expect("supported target count fits u64")),
    )
}
