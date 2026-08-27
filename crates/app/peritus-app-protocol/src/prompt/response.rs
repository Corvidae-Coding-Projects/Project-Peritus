//! Closed unprivileged prompt answer values.

use crate::CorrelationId;

use super::{PromptCorrelation, PromptError, PromptErrorKind, error::reject};

/// Closed user-input response value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UserInputValue {
    /// Bounded UTF-8 text.
    Text(String),
    /// Stable identity of one bound selection.
    Selection(String),
    /// Boolean confirmation.
    Confirmation(bool),
    /// Opaque secret-store reference; plaintext is neither required nor implied.
    SecretReference(String),
}

impl UserInputValue {
    /// Creates bounded text input.
    ///
    /// # Errors
    ///
    /// Rejects a zero limit or oversized text.
    pub fn text(value: String, maximum_bytes: usize) -> Result<Self, PromptError> {
        bounded(value, maximum_bytes, true).map(Self::Text)
    }
    /// Creates a nonempty bounded selection identity.
    ///
    /// # Errors
    ///
    /// Rejects a zero limit or empty/oversized identity.
    pub fn selection(value: String, maximum_bytes: usize) -> Result<Self, PromptError> {
        bounded(value, maximum_bytes, false).map(Self::Selection)
    }
    /// Creates a confirmation value.
    #[must_use]
    pub const fn confirmation(value: bool) -> Self {
        Self::Confirmation(value)
    }
    /// Creates a nonempty bounded opaque secret reference.
    ///
    /// # Errors
    ///
    /// Rejects a zero limit or empty/oversized reference.
    pub fn secret_reference(value: String, maximum_bytes: usize) -> Result<Self, PromptError> {
        bounded(value, maximum_bytes, false).map(Self::SecretReference)
    }
}

fn bounded(value: String, maximum_bytes: usize, allow_empty: bool) -> Result<String, PromptError> {
    if maximum_bytes == 0 {
        return Err(reject(PromptErrorKind::InvalidLimit, "answer text limit is zero"));
    }
    if (!allow_empty && value.is_empty()) || value.len() > maximum_bytes {
        return Err(reject(
            PromptErrorKind::InvalidInput,
            "answer text is empty or exceeds its negotiated bound",
        ));
    }
    Ok(value)
}

/// Closed answer payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PromptAnswerPayload {
    /// Externally signed decision or cancellation plus bounded optional rationale.
    Approval {
        /// Closed signed-decision or cancellation response.
        answer: super::ApprovalAnswer,
        /// Optional bounded human rationale.
        rationale: Option<String>,
    },
    /// One bounded user-input value.
    UserInput(UserInputValue),
}

impl PromptAnswerPayload {
    /// Creates a signed approval response with optional bounded rationale.
    ///
    /// # Errors
    ///
    /// Rejects a zero rationale bound or oversized rationale.
    pub fn signed_approval(
        decision: super::SignedApprovalDecisionFrame,
        rationale: Option<String>,
        maximum_rationale_bytes: usize,
    ) -> Result<Self, PromptError> {
        Self::approval(
            super::ApprovalAnswer::SignedDecision(decision),
            rationale,
            maximum_rationale_bytes,
        )
    }

    /// Creates an unprivileged approval cancellation with optional bounded rationale.
    ///
    /// # Errors
    ///
    /// Rejects a zero rationale bound or oversized rationale.
    pub fn cancel_approval(
        rationale: Option<String>,
        maximum_rationale_bytes: usize,
    ) -> Result<Self, PromptError> {
        Self::approval(super::ApprovalAnswer::Cancel, rationale, maximum_rationale_bytes)
    }

    fn approval(
        answer: super::ApprovalAnswer,
        rationale: Option<String>,
        maximum_rationale_bytes: usize,
    ) -> Result<Self, PromptError> {
        if maximum_rationale_bytes == 0 {
            return Err(reject(PromptErrorKind::InvalidLimit, "rationale limit is zero"));
        }
        if rationale.as_ref().is_some_and(|text| text.len() > maximum_rationale_bytes) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "approval rationale exceeds its negotiated bound",
            ));
        }
        Ok(Self::Approval { answer, rationale })
    }
}

/// One answer echoing the complete immutable prompt correlation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PromptAnswer {
    correlation: PromptCorrelation,
    payload: PromptAnswerPayload,
}

impl PromptAnswer {
    /// Creates a fully correlated answer under an independent negotiated text bound.
    ///
    /// # Errors
    ///
    /// Rejects a zero bound, oversized text, or an empty selection/secret reference.
    pub fn new(
        correlation: PromptCorrelation,
        payload: PromptAnswerPayload,
        maximum_answer_bytes: usize,
    ) -> Result<Self, PromptError> {
        if maximum_answer_bytes == 0 {
            return Err(reject(PromptErrorKind::InvalidLimit, "answer text limit is zero"));
        }
        let text = match &payload {
            PromptAnswerPayload::Approval { rationale, .. } => rationale.as_deref(),
            PromptAnswerPayload::UserInput(
                UserInputValue::Text(text)
                | UserInputValue::Selection(text)
                | UserInputValue::SecretReference(text),
            ) => Some(text.as_str()),
            PromptAnswerPayload::UserInput(UserInputValue::Confirmation(_)) => None,
        };
        if text.is_some_and(|text| text.len() > maximum_answer_bytes) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "answer text exceeds its negotiated bound",
            ));
        }
        if matches!(
            &payload,
            PromptAnswerPayload::UserInput(
                UserInputValue::Selection(text) | UserInputValue::SecretReference(text)
            ) if text.is_empty()
        ) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "selection and secret-reference answers must be nonempty",
            ));
        }
        Ok(Self { correlation, payload })
    }
    /// Returns the echoed complete correlation.
    #[must_use]
    pub const fn correlation(&self) -> PromptCorrelation {
        self.correlation
    }
    /// Borrows the closed answer payload.
    #[must_use]
    pub const fn payload(&self) -> &PromptAnswerPayload {
        &self.payload
    }
}

/// Correlated prompt cancellation fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PromptCancellation {
    correlation: PromptCorrelation,
    correlation_id: CorrelationId,
}

impl PromptCancellation {
    /// Creates a cancellation echoing the complete prompt binding.
    #[must_use]
    pub const fn new(correlation: PromptCorrelation, correlation_id: CorrelationId) -> Self {
        Self { correlation, correlation_id }
    }
    /// Returns the echoed prompt correlation.
    #[must_use]
    pub const fn correlation(self) -> PromptCorrelation {
        self.correlation
    }
    /// Returns the cancellation request/response correlation.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation_id
    }
}
