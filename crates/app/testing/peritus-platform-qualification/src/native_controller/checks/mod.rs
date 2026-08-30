//! Scenario dispatch and shared native observations.

mod daemon;
mod host;
mod lifecycle;
mod package;
mod runtime;

use serde::Serialize;

use super::args::ControllerPaths;
use super::request::BoundRequest;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum CheckOutcome {
    Passed,
    Failed,
    Unsupported,
}

#[derive(Debug, Serialize)]
pub(super) struct Observation {
    pub(super) outcome: CheckOutcome,
    pub(super) summary: String,
    pub(super) facts: Vec<Fact>,
    pub(super) counts: Vec<Count>,
}

#[derive(Debug, Serialize)]
pub(super) struct Fact {
    pub(super) label: String,
    pub(super) value: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct Count {
    pub(super) label: String,
    pub(super) value: u64,
}

impl Observation {
    pub(super) fn passed(summary: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Passed,
            summary: summary.into(),
            facts: vec![Fact { label: "native.assertions-passed".to_owned(), value: true }],
            counts: Vec::new(),
        }
    }

    pub(super) fn failed(summary: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Failed,
            summary: summary.into(),
            facts: vec![Fact { label: "native.assertions-passed".to_owned(), value: false }],
            counts: Vec::new(),
        }
    }

    pub(super) fn unsupported(summary: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Unsupported,
            summary: summary.into(),
            facts: vec![Fact { label: "native.facility-supported".to_owned(), value: false }],
            counts: Vec::new(),
        }
    }

    pub(super) fn fact(mut self, label: impl Into<String>, value: bool) -> Self {
        self.facts.push(Fact { label: label.into(), value });
        self
    }

    pub(super) fn count(mut self, label: impl Into<String>, value: u64) -> Self {
        self.counts.push(Count { label: label.into(), value });
        self
    }
}

pub(super) fn run(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    match request.scenario_id() {
        "artifact-integrity" | "release-layout" | "protected-roots" | "service-autostart" => {
            package::run(paths, request)
        }
        "service-restart"
        | "local-transport"
        | "peer-authentication"
        | "cli-status"
        | "tui-lifecycle"
        | "process-equivalence"
        | "pipe-separation"
        | "terminal-ownership"
        | "cancellation-tree-reap"
        | "sandbox-denial"
        | "sandbox-execution" => runtime::run(paths, request),
        "upgrade-preservation" | "upgrade-rollback" | "uninstall-preservation" => {
            lifecycle::run(paths, request)
        }
        _ => Err("H2 controller received an unknown scenario".into()),
    }
}
