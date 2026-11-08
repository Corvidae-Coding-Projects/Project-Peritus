//! Stateful construction-cadence oracle against the production scheduler state.

use super::{ConstructionProgress, ProgressOutcome};
use proptest::prelude::*;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Inspect,
    CandidateChange,
    ExternalEffect,
    DeliverFeedback,
    Steering,
}

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Observing,
    Feedback,
    Deadline,
    Boundary,
}

#[derive(Default)]
struct Model {
    external_effect_required: bool,
    observations: u16,
    phase: Phase,
}

impl Model {
    fn new(external_effect_required: bool) -> Self {
        Self { external_effect_required, ..Self::default() }
    }

    fn apply(&mut self, operation: Operation) -> Option<bool> {
        match operation {
            Operation::CandidateChange | Operation::ExternalEffect
                if self.external_effect_required =>
            {
                self.observations = 0;
                self.phase = Phase::Observing;
            }
            Operation::CandidateChange => {
                self.observations = 0;
                self.phase = Phase::Observing;
            }
            Operation::ExternalEffect | Operation::Inspect => match self.phase {
                Phase::Observing => {
                    self.observations = self.observations.saturating_add(1);
                    if self.observations >= 8 {
                        self.phase = Phase::Feedback;
                    }
                }
                Phase::Deadline => self.phase = Phase::Boundary,
                Phase::Feedback | Phase::Boundary => {}
            },
            Operation::DeliverFeedback => {
                let delivered = matches!(self.phase, Phase::Feedback);
                if delivered {
                    self.phase = Phase::Deadline;
                }
                return Some(delivered);
            }
            Operation::Steering => {
                self.phase = Phase::Deadline;
            }
        }
        None
    }
}

fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        8 => Just(Operation::Inspect),
        3 => Just(Operation::CandidateChange),
        3 => Just(Operation::ExternalEffect),
        2 => Just(Operation::DeliverFeedback),
        2 => Just(Operation::Steering),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, max_shrink_iters: 16_384, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_histories_match_an_independent_deadline_model(
        external_effect_required in any::<bool>(),
        operations in prop::collection::vec(operation(), 0..160),
    ) {
        let mut actual = ConstructionProgress::enabled(external_effect_required);
        let mut model = Model::new(external_effect_required);
        for (index, operation) in operations.into_iter().enumerate() {
            let expected_feedback = model.apply(operation);
            let actual_feedback = match operation {
                Operation::Inspect => {
                    actual.observe(ProgressOutcome::NoDelivery);
                    None
                }
                Operation::CandidateChange => {
                    actual.observe(ProgressOutcome::CandidateChanged);
                    None
                }
                Operation::ExternalEffect => {
                    actual.observe(ProgressOutcome::ExternalEffect);
                    None
                }
                Operation::DeliverFeedback => Some(actual.take_feedback()),
                Operation::Steering => {
                    actual.steering_continues_work();
                    None
                }
            };
            prop_assert_eq!(actual_feedback, expected_feedback, "feedback mismatch at {}", index);
            prop_assert_eq!(
                actual.segment_boundary_required(),
                matches!(model.phase, Phase::Boundary),
                "boundary mismatch at {}", index,
            );
            prop_assert_eq!(
                actual.guidance().is_empty(),
                matches!(model.phase, Phase::Observing),
                "guidance mismatch at {}", index,
            );
        }
    }
}

#[test]
fn user_steering_requires_the_very_next_tool_batch_to_deliver() {
    let mut progress = ConstructionProgress::enabled(false);
    progress.steering_continues_work();
    assert!(!progress.segment_boundary_required());
    progress.observe(ProgressOutcome::NoDelivery);
    assert!(progress.segment_boundary_required());

    progress.observe(ProgressOutcome::CandidateChanged);
    assert!(!progress.segment_boundary_required());
    assert!(progress.guidance().is_empty());
}

#[test]
fn optional_external_commands_cannot_masquerade_as_candidate_progress() {
    let mut progress = ConstructionProgress::enabled(false);
    for _ in 0..8 {
        progress.observe(ProgressOutcome::ExternalEffect);
    }
    assert!(progress.take_feedback());
    progress.observe(ProgressOutcome::ExternalEffect);
    assert!(progress.segment_boundary_required());

    let mut required = ConstructionProgress::enabled(true);
    for _ in 0..32 {
        required.observe(ProgressOutcome::ExternalEffect);
    }
    assert!(!required.take_feedback());
    assert!(!required.segment_boundary_required());
}
