//! Closed H1 daemon lifecycle phase mapping.

use super::{FaultDocument, ScenarioDocument};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_controller) enum DaemonPhase {
    WriterPending,
    WriterActive,
    GatesPending,
    GatesActive,
    ReviewPending,
    ReviewActive,
    FixerPending,
    FixerActive,
    RevisionAdvancing,
    EvaluatingAcceptance,
    KernelAcceptancePending,
}

impl DaemonPhase {
    pub(in crate::native_controller) const fn code(self) -> &'static str {
        match self {
            Self::WriterPending => "writer-pending",
            Self::WriterActive => "writer-active",
            Self::GatesPending => "gates-pending",
            Self::GatesActive => "gates-active",
            Self::ReviewPending => "review-pending",
            Self::ReviewActive => "review-active",
            Self::FixerPending => "fixer-pending",
            Self::FixerActive => "fixer-active",
            Self::RevisionAdvancing => "revision-advancing",
            Self::EvaluatingAcceptance => "evaluating-acceptance",
            Self::KernelAcceptancePending => "kernel-acceptance-pending",
        }
    }
}

pub(super) fn from_scenario(scenario: &ScenarioDocument) -> Option<DaemonPhase> {
    let FaultDocument::DaemonKill { phase } = &scenario.fault else {
        return None;
    };
    if scenario.expected_recovery != "reconciled-owned-work"
        || scenario.id != format!("h1.daemon-kill.{phase}")
    {
        return None;
    }
    match phase.as_str() {
        "writer-pending" => Some(DaemonPhase::WriterPending),
        "writer-active" => Some(DaemonPhase::WriterActive),
        "gates-pending" => Some(DaemonPhase::GatesPending),
        "gates-active" => Some(DaemonPhase::GatesActive),
        "review-pending" => Some(DaemonPhase::ReviewPending),
        "review-active" => Some(DaemonPhase::ReviewActive),
        "fixer-pending" => Some(DaemonPhase::FixerPending),
        "fixer-active" => Some(DaemonPhase::FixerActive),
        "revision-advancing" => Some(DaemonPhase::RevisionAdvancing),
        "evaluating-acceptance" => Some(DaemonPhase::EvaluatingAcceptance),
        "kernel-acceptance-pending" => Some(DaemonPhase::KernelAcceptancePending),
        _ => None,
    }
}
