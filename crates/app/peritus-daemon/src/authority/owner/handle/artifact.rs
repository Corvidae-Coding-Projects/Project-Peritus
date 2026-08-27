//! Artifact transfer methods on the bounded authority client.

use peritus_app_protocol::{
    ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest,
    TransferId,
};
use peritus_types::{ActorId, SessionId};
use tokio::sync::oneshot;

use super::{AuthorityHandle, AuthorityMessage};
use crate::{DaemonError, artifact::ArtifactPoll};

impl AuthorityHandle {
    /// Opens one exact actor/session-bound artifact download.
    ///
    /// # Errors
    ///
    /// Returns a typed catalog, integrity, or transfer admission failure.
    pub async fn open_artifact(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        request: ArtifactOpenRequest,
        maximum_chunk_bytes: usize,
    ) -> Result<ArtifactMetadata, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::OpenArtifact {
                actor_id,
                session_id,
                request,
                maximum_chunk_bytes,
                respond,
            },
            receive,
        )
        .await
    }

    /// Reads the next chunk or exact completion of one owned artifact download.
    ///
    /// # Errors
    ///
    /// Returns a typed ownership, integrity, or storage failure.
    pub(crate) async fn poll_artifact(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_id: TransferId,
        maximum_chunk_bytes: usize,
    ) -> Result<ArtifactPoll, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::PollArtifact {
                actor_id,
                session_id,
                transfer_id,
                maximum_chunk_bytes,
                respond,
            },
            receive,
        )
        .await
    }

    /// Begins one bounded owned artifact upload.
    ///
    /// # Errors
    ///
    /// Returns a typed catalog, quota, or transfer admission failure.
    pub async fn begin_artifact_upload(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::BeginArtifactUpload {
                actor_id,
                session_id,
                metadata,
                maximum_chunk_bytes,
                respond,
            },
            receive,
        )
        .await
    }

    /// Writes one exact contiguous chunk to an owned upload.
    ///
    /// # Errors
    ///
    /// Returns a typed ordering, ownership, or storage failure.
    pub async fn upload_artifact_chunk(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        chunk: ArtifactChunk,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::UploadArtifactChunk { actor_id, session_id, chunk, respond },
            receive,
        )
        .await
    }

    /// Finalizes one exact upload and publishes its durable application catalog fact.
    ///
    /// # Errors
    ///
    /// Returns a typed conservation, durability, or storage failure.
    pub async fn complete_artifact_upload(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        completion: ArtifactCompletion,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::CompleteArtifactUpload { actor_id, session_id, completion, respond },
            receive,
        )
        .await
    }

    /// Cancels and drops one exact upload or download.
    ///
    /// # Errors
    ///
    /// Returns a typed binding or terminal-state failure.
    pub async fn cancel_artifact_transfer(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        cancellation: ArtifactCancellation,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::CancelArtifactTransfer {
                actor_id,
                session_id,
                cancellation,
                respond,
            },
            receive,
        )
        .await
    }

    pub(crate) async fn abandon_artifact_transfers(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_ids: Vec<TransferId>,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::AbandonArtifactTransfers {
                actor_id,
                session_id,
                transfer_ids,
                respond,
            },
            receive,
        )
        .await
    }
}
