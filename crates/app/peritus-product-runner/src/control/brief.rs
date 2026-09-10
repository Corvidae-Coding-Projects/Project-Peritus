//! User-confirmed brief fields backed by exact immutable input revisions.

use super::{
    ControlError, ControlText, InputId, InputLedger, InputSelection, InputState, OperationId,
    QueueIntent,
};
use peritus_types::ActorId;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Write;

#[cfg(test)]
mod tests;

/// User-editable brief field. Execution mode and observed facts are separate projections.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefField {
    /// Current explicit objective.
    Objective,
    /// User-defined acceptance criteria, not a generated completion claim.
    Acceptance,
    /// User-defined constraints, without additional execution authority.
    Constraints,
    /// Assumptions explicitly confirmed by the user, never inferred from model prose.
    Assumptions,
}
impl BriefField {
    /// Returns the stable human-readable field label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Objective => "objective",
            Self::Acceptance => "acceptance criteria",
            Self::Constraints => "constraints",
            Self::Assumptions => "user-confirmed assumptions",
        }
    }
}

/// Source of one user-confirmed field; held/withdrawn inputs do not become active requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefBinding {
    field: BriefField,
    selected: InputSelection,
}
impl BriefBinding {
    /// Returns the explicit field classification.
    #[must_use]
    pub const fn field(self) -> BriefField {
        self.field
    }
    /// Returns the exact immutable content revision supplying the field.
    #[must_use]
    pub const fn selected(self) -> InputSelection {
        self.selected
    }
}

/// Bounded current brief bindings. Full previous revisions remain in the input/control journal.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskBrief {
    bindings: Vec<BriefBinding>,
}
impl TaskBrief {
    /// Returns whether no user has assigned any brief field.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
    /// Borrows the field-ordered source bindings, including visibly held/withdrawn fields.
    #[must_use]
    pub fn bindings(&self) -> &[BriefBinding] {
        &self.bindings
    }

    /// Borrows the exact latest source for one explicitly confirmed field.
    ///
    /// This does not treat held, withdrawn, or superseded text as an active requirement.
    #[must_use]
    pub fn active_source<'a>(
        &self,
        field: BriefField,
        inputs: &'a InputLedger,
    ) -> Option<&'a super::InputRevision> {
        let binding = self.bindings.iter().find(|binding| binding.field == field)?;
        let source = inputs.latest(binding.selected.id())?;
        matches!(source.state(), InputState::Queued | InputState::Incorporated).then_some(source)
    }

    pub(super) fn set(
        &mut self,
        inputs: &mut InputLedger,
        actor: ActorId,
        operation: OperationId,
        field: BriefField,
        text: &ControlText<8192>,
    ) -> Result<(), ControlError> {
        self.validate(inputs)?;
        let mut key = b"peritus-workbench/brief-input/v1".to_vec();
        key.extend_from_slice(operation.as_bytes());
        let digest = peritus_codec::sha256(&key);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[0] |= 1;
        let fresh = InputId::new(bytes)?;
        let prior = self
            .bindings
            .iter()
            .find(|binding| binding.field == field)
            .map(|binding| binding.selected);
        let (id, intent) = if let Some(selected) = prior {
            let old = inputs.latest(selected.id()).ok_or(ControlError::InvalidInput)?;
            match old.state() {
                InputState::Queued | InputState::Held => {
                    (selected.id(), QueueIntent::Edit { selected, text: text.clone() })
                }
                InputState::Incorporated => (
                    fresh,
                    QueueIntent::Correct { original: selected, id: fresh, text: text.clone() },
                ),
                InputState::Withdrawn => (
                    fresh,
                    QueueIntent::Enqueue {
                        id: fresh,
                        text: text.clone(),
                        dependencies: Vec::new(),
                    },
                ),
                InputState::Superseded => return Err(ControlError::InvalidInput),
            }
        } else {
            (
                fresh,
                QueueIntent::Enqueue { id: fresh, text: text.clone(), dependencies: Vec::new() },
            )
        };
        *inputs = inputs.apply(actor, &intent)?;
        let selected = inputs.latest(id).ok_or(ControlError::InvalidInput)?.selection();
        self.bindings.retain(|binding| binding.field != field);
        self.bindings.push(BriefBinding { field, selected });
        self.bindings.sort_by_key(|binding| binding.field);
        self.validate(inputs)
    }

    pub(super) fn refresh(&mut self, inputs: &InputLedger) -> Result<(), ControlError> {
        // An ordinary queue edit changes the exact source revision without losing its explicit
        // field classification. A correction has a new identity and is not silently promoted.
        for binding in &mut self.bindings {
            binding.selected =
                inputs.latest(binding.selected.id()).ok_or(ControlError::InvalidInput)?.selection();
        }
        self.validate(inputs)
    }

    pub(super) fn validate(&self, inputs: &InputLedger) -> Result<(), ControlError> {
        if self.bindings.len() > 4
            || self.bindings.windows(2).any(|pair| pair[0].field >= pair[1].field)
            || self.bindings.iter().any(|binding| {
                inputs
                    .latest(binding.selected.id())
                    .is_none_or(|input| input.selection() != binding.selected)
            })
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }

    pub(in crate::control) fn append_to(
        &self,
        text: &mut String,
        included: &[InputSelection],
        inputs: &InputLedger,
    ) -> Result<(), ControlError> {
        self.validate(inputs)?;
        let mut fields = String::new();
        for binding in self.bindings.iter().filter(|binding| included.contains(&binding.selected)) {
            let source = inputs.latest(binding.selected.id()).ok_or(ControlError::InvalidInput)?;
            write!(
                fields,
                "\n{} (input content revision {}):\n{}\n",
                binding.field.label(),
                binding.selected.revision(),
                source.text()
            )
            .map_err(|_| ControlError::Capacity)?;
        }
        if !fields.is_empty() {
            let heading = "\n\nCurrent user-confirmed brief. These explicitly edited fields supersede earlier values of the same fields; historical inputs remain archived. Confirmed assumptions do not grant tool permissions.\n";
            if text.len().saturating_add(heading.len()).saturating_add(fields.len()) > 1024 * 1024 {
                return Err(ControlError::Capacity);
            }
            text.push_str(heading);
            text.push_str(&fields);
        }
        Ok(())
    }
}
