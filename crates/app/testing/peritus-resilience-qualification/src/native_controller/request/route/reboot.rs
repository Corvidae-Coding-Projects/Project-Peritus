//! Closed H1 disposable-host reboot route identities.

use super::super::{FaultDocument, ScenarioDocument};

pub(in crate::native_controller::request) const REBOOT_OUTSTANDING_EFFECT: &str =
    "h1.reboot.outstanding-effect";
pub(in crate::native_controller::request) const REBOOT_DURABLE_BEFORE_ACK: &str =
    "h1.reboot.durable-before-ack";
pub(in crate::native_controller::request) const REBOOT_STARTUP_RECONCILIATION: &str =
    "h1.reboot.startup-reconciliation";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_controller) enum RebootPhase {
    OutstandingEffect,
    DurableBeforeAck,
    StartupReconciliation,
}

impl RebootPhase {
    pub(in crate::native_controller) const fn code(self) -> &'static str {
        match self {
            Self::OutstandingEffect => "outstanding-effect",
            Self::DurableBeforeAck => "durable-before-ack",
            Self::StartupReconciliation => "startup-reconciliation",
        }
    }
}

pub(super) fn from_scenario(scenario: &ScenarioDocument) -> Option<RebootPhase> {
    match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
        (
            REBOOT_OUTSTANDING_EFFECT,
            "reconciled-owned-work",
            FaultDocument::HostReboot { phase },
        ) if phase == "outstanding-effect" => Some(RebootPhase::OutstandingEffect),
        (
            REBOOT_DURABLE_BEFORE_ACK,
            "reconciled-owned-work",
            FaultDocument::HostReboot { phase },
        ) if phase == "durable-before-ack" => Some(RebootPhase::DurableBeforeAck),
        (
            REBOOT_STARTUP_RECONCILIATION,
            "reconciled-owned-work",
            FaultDocument::HostReboot { phase },
        ) if phase == "startup-reconciliation" => Some(RebootPhase::StartupReconciliation),
        _ => None,
    }
}
