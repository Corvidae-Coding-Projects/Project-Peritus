//! Per-connection transfer ownership and bounded download event pumping.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Bound::{Excluded, Unbounded},
};

use peritus_app_protocol::{
    AppEventEnvelope, AppMessage, AppProtocolLimits, ArtifactMetadata, ProtocolContext, TransferId,
};
use peritus_types::{ActorId, ArtifactId, SessionId};

use crate::{AppFrameStream, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

const MAX_DOWNLOAD_POLLS_PER_TICK: usize = 16;
const MAX_ABANDONMENTS_PER_BATCH: usize = 256;

#[derive(Clone, Copy)]
enum Direction {
    Download,
    Upload,
}

pub struct ArtifactClient {
    transfers: BTreeMap<TransferId, (ArtifactId, Direction)>,
    downloads: BTreeSet<TransferId>,
    last_download_polled: Option<TransferId>,
}

impl ArtifactClient {
    pub(crate) const fn new() -> Self {
        Self {
            transfers: BTreeMap::new(),
            downloads: BTreeSet::new(),
            last_download_polled: None,
        }
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
        self.downloads.remove(&transfer_id);
    }

    pub(crate) fn transfer_batch(&self, after: Option<TransferId>) -> Vec<TransferId> {
        // This is an authority-message page bound, not a cumulative transfer allowance.
        match after {
            Some(after) => self
                .transfers
                .range((Excluded(after), Unbounded))
                .map(|(transfer_id, _)| *transfer_id)
                .take(MAX_ABANDONMENTS_PER_BATCH)
                .collect(),
            None => self
                .transfers
                .keys()
                .copied()
                .take(MAX_ABANDONMENTS_PER_BATCH)
                .collect(),
        }
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
        let downloads = self.next_download_batch();
        for transfer_id in downloads {
            let poll = match authority
                .poll_artifact(actor_id, session_id, transfer_id, limits.max_artifact_chunk_bytes())
                .await
            {
                Ok(poll) => poll,
                // Keep this logical transfer admitted and rotate to the next one. The authority
                // classifies physical contention as retryable capacity, so a busy old transfer
                // must not tear down the connection or block a newly eligible transfer.
                Err(error) if error.code_kind() == DaemonErrorCode::ResourceLimit => continue,
                Err(error) => return Err(error),
            };
            frames.write(&AppMessage::Event(AppEventEnvelope::new(context, poll.payload))).await?;
            if poll.terminal {
                self.remove(transfer_id);
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
        self.transfers.insert(metadata.transfer_id(), (metadata.artifact_id(), direction));
        if matches!(direction, Direction::Download) {
            self.downloads.insert(metadata.transfer_id());
        }
        Ok(())
    }

    fn next_download_batch(&mut self) -> Vec<TransferId> {
        // The disjoint ranges start after the last attempted identity and wrap once. A batch
        // therefore allocates and polls at most the physical per-tick bound while every admitted
        // download advances across successive ticks.
        let downloads = match self.last_download_polled {
            Some(last) => self
                .downloads
                .range((Excluded(last), Unbounded))
                .chain(self.downloads.range(..=last))
                .copied()
                .take(MAX_DOWNLOAD_POLLS_PER_TICK)
                .collect::<Vec<_>>(),
            None => self
                .downloads
                .iter()
                .copied()
                .take(MAX_DOWNLOAD_POLLS_PER_TICK)
                .collect(),
        };
        if let Some(last) = downloads.last() {
            self.last_download_polled = Some(*last);
        }
        downloads
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
