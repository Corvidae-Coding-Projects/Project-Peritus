//! Host-owned construction cadence based on observed candidate progress.

const OBSERVATION_ALLOWANCE: u16 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProgressOutcome {
    CandidateChanged,
    ExternalEffect,
    NoDelivery,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ConstructionMode {
    #[default]
    Disabled,
    Workspace,
    RequiredExternalEffect,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Cadence {
    #[default]
    Observing,
    FeedbackPending,
    DeliveryRequired,
    SegmentBoundary,
}

#[derive(Default)]
pub(super) struct ConstructionProgress {
    mode: ConstructionMode,
    cadence: Cadence,
    calls_without_delivery: u16,
}

impl ConstructionProgress {
    pub(super) const fn enabled(external_effect_required: bool) -> Self {
        Self {
            mode: if external_effect_required {
                ConstructionMode::RequiredExternalEffect
            } else {
                ConstructionMode::Workspace
            },
            cadence: Cadence::Observing,
            calls_without_delivery: 0,
        }
    }

    pub(super) fn observe(&mut self, outcome: ProgressOutcome) {
        if self.mode == ConstructionMode::Disabled {
            return;
        }
        let delivered = outcome == ProgressOutcome::CandidateChanged
            || (outcome == ProgressOutcome::ExternalEffect
                && self.mode == ConstructionMode::RequiredExternalEffect);
        if delivered {
            self.calls_without_delivery = 0;
            self.cadence = Cadence::Observing;
            return;
        }
        match self.cadence {
            Cadence::Observing => {
                self.calls_without_delivery = self.calls_without_delivery.saturating_add(1);
                if self.calls_without_delivery >= OBSERVATION_ALLOWANCE {
                    self.cadence = Cadence::FeedbackPending;
                }
            }
            Cadence::FeedbackPending | Cadence::SegmentBoundary => {}
            Cadence::DeliveryRequired => self.cadence = Cadence::SegmentBoundary,
        }
    }

    pub(super) const fn steering_continues_work(&mut self) {
        if matches!(self.mode, ConstructionMode::Disabled) {
            return;
        }
        self.cadence = Cadence::DeliveryRequired;
    }

    pub(super) fn take_feedback(&mut self) -> bool {
        if self.cadence != Cadence::FeedbackPending {
            return false;
        }
        self.cadence = Cadence::DeliveryRequired;
        true
    }

    pub(super) const fn segment_boundary_required(&self) -> bool {
        matches!(self.cadence, Cadence::SegmentBoundary)
    }

    pub(super) const fn guidance(&self) -> &'static str {
        if !matches!(self.mode, ConstructionMode::Disabled)
            && !matches!(self.cadence, Cadence::Observing)
        {
            "CONSTRUCTION DELIVERY DEADLINE: The next tool-bearing response must produce an observed workspace candidate change or, only when the original task requires it, the requested live external effect. Another read, search, context recovery, source checkout, build, test, or unchanged write does not extend this deadline. You may instead return the selected capability's truthful draft handoff or one material question. A no-progress tool batch hands scheduling to another construction segment."
        } else {
            ""
        }
    }
}

#[cfg(test)]
#[path = "../../../../../../discovery/proptest-praxis/owner/construction_progress.rs"]
mod praxis_construction_progress;
