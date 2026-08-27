//! Prompt ownership methods on the bounded authority client.

use peritus_app_protocol::{
    PromptAnswer, PromptBinding, PromptCancellation, PromptCorrelation, RequestId,
};
use peritus_types::{ActorId, SessionId};
use tokio::sync::oneshot;

use super::{AuthorityHandle, AuthorityMessage};
use crate::{DaemonError, PromptTerminalStatus};

impl AuthorityHandle {
    /// Registers one actor/session-owned prompt before it is emitted to the client.
    ///
    /// `maximum_answer_bytes` is the exact negotiated A3 answer ceiling for this prompt.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, ownership, capacity, binding, or protocol failure.
    pub async fn register_prompt(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        binding: PromptBinding,
        maximum_answer_bytes: usize,
    ) -> Result<PromptTerminalStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::RegisterPrompt {
                actor_id,
                session_id,
                binding,
                maximum_answer_bytes,
                respond,
            },
            receive,
        )
        .await
    }

    /// Durably settles one exact fresh prompt answer before terminalizing the live broker entry.
    ///
    /// `request_frame` must be the canonical A3 request containing `request_id` and `answer`.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, ownership, freshness, authentication, or storage failure.
    pub async fn answer_prompt(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        request_id: RequestId,
        answer: PromptAnswer,
        request_frame: Vec<u8>,
    ) -> Result<PromptTerminalStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::AnswerPrompt {
                actor_id,
                session_id,
                request_id,
                answer,
                request_frame,
                respond,
            },
            receive,
        )
        .await
    }

    /// Durably settles one exact fresh prompt cancellation before terminalizing the live broker.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, ownership, freshness, binding, or storage failure.
    pub async fn cancel_prompt(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        request_id: RequestId,
        cancellation: PromptCancellation,
        request_frame: Vec<u8>,
    ) -> Result<PromptTerminalStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::CancelPrompt {
                actor_id,
                session_id,
                request_id,
                cancellation,
                request_frame,
                respond,
            },
            receive,
        )
        .await
    }

    /// Reads the exact retained status of one authenticated actor/session-owned prompt.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, ownership, absence, or binding failure.
    pub async fn prompt_status(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        correlation: PromptCorrelation,
    ) -> Result<PromptTerminalStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::PromptStatus { actor_id, session_id, correlation, respond },
            receive,
        )
        .await
    }

    /// Retires one exact terminal prompt after its authoritative target has settled durably.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, ownership, absence, binding, or still-awaiting failure.
    pub async fn retire_prompt(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        correlation: PromptCorrelation,
    ) -> Result<PromptTerminalStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::RetirePrompt { actor_id, session_id, correlation, respond },
            receive,
        )
        .await
    }

    /// Lists every exact retained prompt correlation for one authenticated actor/session.
    ///
    /// The call fails instead of truncating when the exact result exceeds `maximum`.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, invalid-bound, or resource-limit failure.
    pub async fn prompt_correlations(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        maximum: usize,
    ) -> Result<Vec<PromptCorrelation>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::PromptCorrelations { actor_id, session_id, maximum, respond },
            receive,
        )
        .await
    }
}
