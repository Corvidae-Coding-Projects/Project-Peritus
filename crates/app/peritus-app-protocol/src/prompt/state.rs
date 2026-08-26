//! Pure one-answer prompt admission state.

use peritus_types::RevisionTuple;

use super::{
    PromptAnswer, PromptAnswerPayload, PromptBinding, PromptCancellation, PromptConstraint,
    PromptError, PromptErrorKind, PromptKind, UserInputValue, error::reject,
};

/// Observable prompt lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromptPhase {
    /// One matching fresh answer or cancellation may be accepted.
    AwaitingAnswer,
    /// Exactly one matching fresh answer was accepted.
    Answered(PromptAnswer),
    /// A matching cancellation was accepted.
    Cancelled(PromptCancellation),
}

/// Pure prompt correlation and freshness state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptState {
    binding: PromptBinding,
    maximum_answer_bytes: usize,
    phase: PromptPhase,
}

impl PromptState {
    /// Creates an awaiting state with an independent negotiated answer-text bound.
    ///
    /// # Errors
    ///
    /// Rejects a zero answer-text bound.
    pub fn new(binding: PromptBinding, maximum_answer_bytes: usize) -> Result<Self, PromptError> {
        if maximum_answer_bytes == 0 {
            return Err(reject(PromptErrorKind::InvalidLimit, "answer text limit is zero"));
        }
        Ok(Self { binding, maximum_answer_bytes, phase: PromptPhase::AwaitingAnswer })
    }

    /// Borrows the immutable prompt binding.
    #[must_use]
    pub const fn binding(&self) -> &PromptBinding {
        &self.binding
    }
    /// Borrows the current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> &PromptPhase {
        &self.phase
    }

    /// Accepts exactly one answer matching the complete binding and caller-supplied live revision.
    ///
    /// This validates correlation and freshness only. It does not authenticate the actor or create,
    /// sign, or consume a B1 approval.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, correlation mismatch, stale revision, wrong answer kind, unknown
    /// choice, or violation of a bound input constraint.
    pub fn answer(
        &mut self,
        answer: PromptAnswer,
        live_revision: RevisionTuple,
    ) -> Result<(), PromptError> {
        if self.phase != PromptPhase::AwaitingAnswer {
            return Err(reject(PromptErrorKind::AlreadyTerminal, "prompt is already terminal"));
        }
        if answer.correlation() != self.binding.correlation() {
            return Err(reject(
                PromptErrorKind::BindingMismatch,
                "answer does not echo the complete prompt correlation",
            ));
        }
        if live_revision != self.binding.correlation().revision() {
            return Err(reject(
                PromptErrorKind::StaleRevision,
                "live revision differs from the prompt revision",
            ));
        }
        self.validate_payload(answer.payload())?;
        self.phase = PromptPhase::Answered(answer);
        Ok(())
    }

    /// Applies one cancellation matching the complete prompt correlation.
    ///
    /// # Errors
    ///
    /// Rejects terminal state or any correlation mismatch.
    pub fn cancel(&mut self, cancellation: PromptCancellation) -> Result<(), PromptError> {
        if self.phase != PromptPhase::AwaitingAnswer {
            return Err(reject(PromptErrorKind::AlreadyTerminal, "prompt is already terminal"));
        }
        if cancellation.correlation() != self.binding.correlation() {
            return Err(reject(
                PromptErrorKind::BindingMismatch,
                "cancellation does not echo the complete prompt correlation",
            ));
        }
        self.phase = PromptPhase::Cancelled(cancellation);
        Ok(())
    }

    fn validate_payload(&self, payload: &PromptAnswerPayload) -> Result<(), PromptError> {
        match (self.binding.kind(), payload) {
            (PromptKind::Approval, PromptAnswerPayload::Approval { rationale, .. }) => {
                if rationale.as_ref().is_some_and(|text| text.len() > self.maximum_answer_bytes) {
                    Err(reject(
                        PromptErrorKind::InvalidInput,
                        "approval rationale exceeds the negotiated answer bound",
                    ))
                } else {
                    Ok(())
                }
            }
            (PromptKind::UserInput, PromptAnswerPayload::UserInput(value)) => {
                self.validate_user_input(value)
            }
            _ => Err(reject(
                PromptErrorKind::WrongAnswerKind,
                "answer payload does not match the bound prompt kind",
            )),
        }
    }

    fn validate_user_input(&self, value: &UserInputValue) -> Result<(), PromptError> {
        let text = match value {
            UserInputValue::Text(text)
            | UserInputValue::Selection(text)
            | UserInputValue::SecretReference(text) => Some(text.as_str()),
            UserInputValue::Confirmation(_) => None,
        };
        if text.is_some_and(|text| text.len() > self.maximum_answer_bytes) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "user input exceeds the negotiated answer bound",
            ));
        }
        if let UserInputValue::Selection(selected) = value
            && !self.binding.choices().iter().any(|choice| choice.id() == selected)
        {
            return Err(reject(
                PromptErrorKind::UnknownChoice,
                "selected option is not present in the bound choice set",
            ));
        }
        for constraint in self.binding.constraints() {
            match constraint {
                PromptConstraint::NonEmpty if matches!(value, UserInputValue::Text(text) if text.is_empty()) =>
                {
                    return Err(reject(
                        PromptErrorKind::InvalidInput,
                        "text answer violates the nonempty constraint",
                    ));
                }
                PromptConstraint::MaximumTextBytes(maximum) => {
                    let limit = usize::try_from(*maximum).map_err(|_| {
                        reject(
                            PromptErrorKind::InvalidInput,
                            "prompt-specific text bound is not representable",
                        )
                    })?;
                    if matches!(value, UserInputValue::Text(text) if text.len() > limit) {
                        return Err(reject(
                            PromptErrorKind::InvalidInput,
                            "text answer violates its prompt-specific byte bound",
                        ));
                    }
                }
                PromptConstraint::BoundChoiceOnly
                    if !matches!(value, UserInputValue::Selection(_)) =>
                {
                    return Err(reject(
                        PromptErrorKind::WrongAnswerKind,
                        "prompt requires a bound selection",
                    ));
                }
                PromptConstraint::SecretReference
                    if !matches!(value, UserInputValue::SecretReference(_)) =>
                {
                    return Err(reject(
                        PromptErrorKind::WrongAnswerKind,
                        "prompt requires an opaque secret reference",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
