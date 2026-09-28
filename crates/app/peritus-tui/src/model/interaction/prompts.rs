//! Prompt composition, validation, cancellation, and submission state.

use super::*;

impl AppModel {
    pub(super) fn open_prompt_editor(&mut self) {
        let Some(item) = self.selected_prompt_item() else {
            return;
        };
        if !matches!(item.phase, PromptPhase::Pending | PromptPhase::Failed) {
            return;
        }
        let prompt_id = item.binding.correlation().prompt_id();
        self.open_editor(match item.binding.kind() {
            PromptKind::Approval => Editor {
                kind: EditorKind::ApprovalSignature(prompt_id),
                title: "Submit externally signed approval decision",
                hint: "Paste the base64-encoded canonical B1 signed-decision frame",
                buffer: String::new(),
                cursor: 0,
            },
            PromptKind::UserInput => Editor {
                kind: EditorKind::PromptAnswer(prompt_id),
                title: "Answer daemon prompt",
                hint: "Enter text, an exact choice id, or an opaque secret reference as required",
                buffer: String::new(),
                cursor: 0,
            },
        });
    }

    pub(in crate::model) fn submit_signed_approval(
        &mut self,
        prompt_id: PromptId,
        encoded: &str,
    ) -> Vec<Effect> {
        let decoded = match BASE64.decode(encoded.trim()) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.notice(NoticeLevel::Error, format!("invalid base64 decision: {error}"));
                return Vec::new();
            }
        };
        let decision =
            match SignedApprovalDecisionFrame::new(decoded, self.limits.codec().max_frame_bytes) {
                Ok(decision) => decision,
                Err(error) => {
                    self.notice(NoticeLevel::Error, format!("invalid signed decision: {error}"));
                    return Vec::new();
                }
            };
        let payload = match PromptAnswerPayload::signed_approval(
            decision,
            None,
            self.limits.max_diagnostic_bytes(),
        ) {
            Ok(payload) => payload,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        self.submit_prompt_answer(prompt_id, payload)
    }

    pub(in crate::model) fn submit_user_input(
        &mut self,
        prompt_id: PromptId,
        value: String,
    ) -> Vec<Effect> {
        let Some(item) = self.prompt(prompt_id) else {
            self.notice(NoticeLevel::Warning, "This prompt is no longer available. Draft retained; select a current prompt in Approvals.");
            return Vec::new();
        };
        let maximum = self.limits.codec().max_string_bytes;
        let input = if item
            .binding
            .constraints()
            .contains(&peritus_app_protocol::PromptConstraint::SecretReference)
        {
            UserInputValue::secret_reference(value, maximum)
        } else if !item.binding.choices().is_empty()
            || item
                .binding
                .constraints()
                .contains(&peritus_app_protocol::PromptConstraint::BoundChoiceOnly)
        {
            UserInputValue::selection(value, maximum)
        } else {
            UserInputValue::text(value, maximum)
        };
        match input {
            Ok(input) => {
                self.submit_prompt_answer(prompt_id, PromptAnswerPayload::UserInput(input))
            }
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                Vec::new()
            }
        }
    }

    fn submit_prompt_answer(
        &mut self,
        prompt_id: PromptId,
        payload: PromptAnswerPayload,
    ) -> Vec<Effect> {
        let Some(binding) = self.prompt(prompt_id).map(|item| item.binding.clone()) else {
            self.notice(NoticeLevel::Warning, "This prompt is no longer available. Draft retained; select a current prompt in Approvals.");
            return Vec::new();
        };
        let answer = match PromptAnswer::new(
            binding.correlation(),
            payload,
            self.limits.codec().max_string_bytes,
        ) {
            Ok(answer) => answer,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        let effect = self
            .request(AppRequestPayload::AnswerPrompt(answer), PendingRequest::Prompt(prompt_id));
        if effect.is_some() {
            self.set_prompt_phase(prompt_id, PromptPhase::Submitting);
        }
        effect.into_iter().collect()
    }

    pub(super) fn cancel_selected_prompt(&mut self) -> Vec<Effect> {
        let Some(binding) = self.selected_prompt_item().map(|item| item.binding.clone()) else {
            return Vec::new();
        };
        let prompt_id = binding.correlation().prompt_id();
        if binding.kind() == PromptKind::Approval {
            let payload = match PromptAnswerPayload::cancel_approval(
                None,
                self.limits.max_diagnostic_bytes(),
            ) {
                Ok(payload) => payload,
                Err(error) => {
                    self.notice(NoticeLevel::Error, error.to_string());
                    return Vec::new();
                }
            };
            return self.submit_prompt_answer(prompt_id, payload);
        }
        let Some(context) = self.context else {
            return Vec::new();
        };
        let (Some(request), Some(correlation)) = (self.ids.request(), self.ids.correlation())
        else {
            return Vec::new();
        };
        let cancellation = PromptCancellation::new(binding.correlation(), correlation);
        let Ok(envelope) = AppRequestEnvelope::new(
            context,
            request,
            correlation,
            AppRequestPayload::CancelPrompt(cancellation),
        ) else {
            return Vec::new();
        };
        self.track_request(request, PendingRequest::Prompt(prompt_id));
        self.set_prompt_phase(prompt_id, PromptPhase::Submitting);
        vec![Effect::Send(AppMessage::Request(envelope))]
    }
}
