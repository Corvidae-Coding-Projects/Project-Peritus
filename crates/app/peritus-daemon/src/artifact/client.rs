//! Per-connection transfer ownership and bounded download event pumping.

use std::collections::BTreeMap;

use peritus_app_protocol::{
    AppEventEnvelope, AppMessage, AppProtocolLimits, ArtifactMetadata, ProtocolContext, TransferId,
};
use peritus_types::{ActorId, ArtifactId, SessionId};

use crate::{AppFrameStream, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

const MAX_DOWNLOAD_POLLS_PER_TICK: usize = 16;

#[derive(Clone, Copy)]
enum Direction {
    Download,
    Upload,
}

pub struct ArtifactClient {
    transfers: BTreeMap<TransferId, (ArtifactId, Direction)>,
    maximum: usize,
}

impl ArtifactClient {
    pub(crate) const fn new(limits: AppProtocolLimits) -> Self {
        Self { transfers: BTreeMap::new(), maximum: limits.max_idempotency_entries() }
    }

    pub(crate) fn register_download(
        &mut self,
        metadata: &ArtifactMetadata,
    ) -> Result<(), DaemonError> {
        self.register(metadata, Direction::Download)
    }

    pub(crate) fn register_upload(
        &mut self,
        metadata: &ArtifactMetadata,
    ) -> Result<(), DaemonError> {
        self.register(metadata, Direction::Upload)
    }

    pub(crate) fn remove(&mut self, transfer_id: TransferId) {
        self.transfers.remove(&transfer_id);
    }

    pub(crate) fn transfer_ids(&self) -> Vec<TransferId> {
        self.transfers.keys().copied().collect()
    }

    pub(crate) async fn pump<S>(
        &mut self,
        frames: &mut AppFrameStream<S>,
        authority: &AuthorityHandle,
        actor_id: ActorId,
        session_id: SessionId,
        context: ProtocolContext,
        limits: AppProtocolLimits,
    ) -> Result<(), DaemonError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let downloads = self
            .transfers
            .iter()
            .filter_map(|(transfer, (_, direction))| {
                matches!(direction, Direction::Download).then_some(*transfer)
            })
            .take(MAX_DOWNLOAD_POLLS_PER_TICK)
            .collect::<Vec<_>>();
        for transfer_id in downloads {
            let poll = authority
                .poll_artifact(actor_id, session_id, transfer_id, limits.max_artifact_chunk_bytes())
                .await?;
            frames.write(&AppMessage::Event(AppEventEnvelope::new(context, poll.payload))).await?;
            if poll.terminal {
                self.transfers.remove(&transfer_id);
            }
        }
        Ok(())
    }

    fn register(
        &mut self,
        metadata: &ArtifactMetadata,
        direction: Direction,
    ) -> Result<(), DaemonError> {
        if self.transfers.contains_key(&metadata.transfer_id()) {
            return Err(invalid("artifact transfer identity is already active on this connection"));
        }
        if self.transfers.len() >= self.maximum {
            return Err(DaemonError::new(
                DaemonErrorCode::ResourceLimit,
                DaemonRecovery::Retry,
                "register connection artifact transfer",
                "connection artifact transfer registry is full",
            ));
        }
        self.transfers.insert(metadata.transfer_id(), (metadata.artifact_id(), direction));
        Ok(())
    }
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "register connection artifact transfer",
        detail,
    )
}
