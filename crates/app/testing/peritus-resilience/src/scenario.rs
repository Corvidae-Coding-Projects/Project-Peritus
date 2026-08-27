//! Immutable scenario definitions.

use crate::{FaultInjection, QualificationText, RecoveryOutcome, ScenarioId};

/// One exact black-box resilience scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioSpec {
    id: ScenarioId,
    title: QualificationText,
    fault: FaultInjection,
    expected_recovery: RecoveryOutcome,
}

impl ScenarioSpec {
    /// Creates a custom scenario definition.
    #[must_use]
    pub const fn new(
        id: ScenarioId,
        title: QualificationText,
        fault: FaultInjection,
        expected_recovery: RecoveryOutcome,
    ) -> Self {
        Self { id, title, fault, expected_recovery }
    }

    /// Returns the stable scenario identifier.
    #[must_use]
    pub const fn id(&self) -> &ScenarioId {
        &self.id
    }
    /// Returns the bounded human-readable title.
    #[must_use]
    pub const fn title(&self) -> &QualificationText {
        &self.title
    }
    /// Returns the exact fault to inject.
    #[must_use]
    pub const fn fault(&self) -> FaultInjection {
        self.fault
    }
    /// Returns the required recovery classification.
    #[must_use]
    pub const fn expected_recovery(&self) -> RecoveryOutcome {
        self.expected_recovery
    }
}
