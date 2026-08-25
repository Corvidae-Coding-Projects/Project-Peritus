//! Deterministic command/event cursor used by lifecycle and durability tests.

use crate::{
    OrchestratorCommand, OrchestratorCommandKind, OrchestratorError, OrchestratorState,
    OrchestratorTransition, decide, start,
};
use peritus_types::{CommandId, EventId};

use super::bytes;

#[derive(Clone)]
pub struct Scenario {
    state: OrchestratorState,
    steps: Vec<(OrchestratorCommand, OrchestratorTransition)>,
    next_identity: u16,
}

impl Scenario {
    pub fn new() -> Self {
        let (command, _, expected) = crate::wire::fixture_tests::values();
        let transition = start(&command).expect("checked genesis starts");
        assert_eq!(transition.state(), &expected);
        Self {
            state: transition.state().clone(),
            steps: vec![(command, transition)],
            next_identity: 100,
        }
    }

    pub const fn state(&self) -> &OrchestratorState {
        &self.state
    }

    pub fn steps(&self) -> &[(OrchestratorCommand, OrchestratorTransition)] {
        &self.steps
    }

    pub fn events(&self) -> Vec<crate::OrchestratorEvent> {
        self.steps.iter().map(|(_, step)| step.event().clone()).collect()
    }

    pub fn next_event_id(&self) -> EventId {
        EventId::new(bytes(self.next_identity + 1)).expect("event identity is nonzero")
    }

    pub fn apply(
        &mut self,
        kind: OrchestratorCommandKind,
    ) -> Result<OrchestratorTransition, OrchestratorError> {
        let command_id =
            CommandId::new(bytes(self.next_identity)).expect("command identity is nonzero");
        let event_id = self.next_event_id();
        self.next_identity += 2;
        let command = OrchestratorCommand::new(
            command_id,
            event_id,
            self.state.binding().run_id(),
            self.state.sequence().get(),
            Some(self.state.last_event_id()),
            self.state.state_digest(),
            self.state.current_candidate().revision(),
            kind,
        )?;
        let transition = decide(&self.state, &command)?;
        self.state = transition.state().clone();
        self.steps.push((command, transition.clone()));
        Ok(transition)
    }

    pub fn apply_ok(&mut self, kind: OrchestratorCommandKind) {
        self.apply(kind).expect("fixture transition is legal");
    }
}
