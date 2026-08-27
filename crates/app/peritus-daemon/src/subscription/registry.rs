//! Closed application topic registry.

use peritus_app_protocol::SubscriptionFilter;
use peritus_journal::{AggregateKind, CommittedRecord};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Topic {
    All,
    EventFamily(u16),
    Aggregate(AggregateKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompiledFilter(Vec<Topic>);

impl CompiledFilter {
    pub(super) fn compile(filter: &SubscriptionFilter) -> Result<Self, DaemonError> {
        let topics = filter
            .topics()
            .iter()
            .map(|topic| compile_topic(topic))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(topics))
    }

    pub(super) fn matches(&self, record: &CommittedRecord) -> bool {
        self.0.iter().any(|topic| match topic {
            Topic::All => true,
            Topic::EventFamily(family) => record.frame_family() == *family,
            Topic::Aggregate(kind) => record.aggregate().kind() == *kind,
        })
    }
}

fn compile_topic(topic: &str) -> Result<Topic, DaemonError> {
    if topic == "system.all" {
        return Ok(Topic::All);
    }
    if let Some(value) = topic.strip_prefix("event.") {
        if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
            return Err(invalid_topic());
        }
        let family = value.parse::<u16>().map_err(|_| invalid_topic())?;
        if family == 0 {
            return Err(invalid_topic());
        }
        return Ok(Topic::EventFamily(family));
    }
    if let Some(value) = topic.strip_prefix("aggregate.") {
        return aggregate_kind(value).map(Topic::Aggregate).ok_or_else(invalid_topic);
    }
    Err(invalid_topic())
}

fn aggregate_kind(value: &str) -> Option<AggregateKind> {
    Some(match value {
        "kernel" => AggregateKind::Kernel,
        "budget" => AggregateKind::Budget,
        "lease" => AggregateKind::Lease,
        "approval" => AggregateKind::Approval,
        "credential-registry" => AggregateKind::CredentialRegistry,
        "agent" => AggregateKind::Agent,
        "gate" => AggregateKind::Gate,
        "trace" => AggregateKind::Trace,
        "review" => AggregateKind::Review,
        "scheduler" => AggregateKind::Scheduler,
        "collaboration" => AggregateKind::Collaboration,
        "orchestrator" => AggregateKind::Orchestrator,
        "harness" => AggregateKind::Harness,
        "debugger" => AggregateKind::Debugger,
        "evaluation" => AggregateKind::Evaluation,
        "evolution-campaign" => AggregateKind::EvolutionCampaign,
        "production-harness" => AggregateKind::ProductionHarness,
        "application" => AggregateKind::Application,
        _ => return None,
    })
}

fn invalid_topic() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "compile subscription filter",
        "subscription topic is not present in the closed daemon topic registry",
    )
}
