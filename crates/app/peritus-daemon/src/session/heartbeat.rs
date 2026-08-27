//! Per-connection monotonic A3 heartbeat exchange.

use peritus_app_protocol::{
    AppEventEnvelope, AppEventPayload, AppMessage, DaemonHeartbeat, DaemonStatus, HeartbeatId,
    HeartbeatReply, ProtocolContext,
};
use sha2::{Digest, Sha256};

use crate::{AppFrameStream, DaemonError, DaemonErrorCode, DaemonRecovery};

const MAX_MISSED_REPLIES: u8 = 3;

/// Exact pending heartbeat state for one authenticated connection.
pub(super) struct ConnectionHeartbeat {
    context: ProtocolContext,
    next_sequence: u64,
    pending: Option<(HeartbeatId, u64)>,
    missed: u8,
}

impl ConnectionHeartbeat {
    pub(super) const fn new(context: ProtocolContext) -> Self {
        Self { context, next_sequence: 1, pending: None, missed: 0 }
    }

    pub(super) async fn send<S>(
        &mut self,
        frames: &mut AppFrameStream<S>,
        status: DaemonStatus,
    ) -> Result<(), DaemonError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        if self.pending.is_some() {
            self.missed = self.missed.checked_add(1).ok_or_else(heartbeat_failure)?;
            if self.missed >= MAX_MISSED_REPLIES {
                return Err(heartbeat_failure());
            }
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1).ok_or_else(heartbeat_failure)?;
        let heartbeat_id = derive_id(self.context, sequence)?;
        self.pending = Some((heartbeat_id, sequence));
        let event = AppEventEnvelope::new(
            self.context,
            AppEventPayload::Heartbeat(DaemonHeartbeat::new(heartbeat_id, sequence, status)),
        );
        frames.write(&AppMessage::Event(event)).await
    }

    pub(super) fn observe(&mut self, reply: HeartbeatReply) -> Result<(), DaemonError> {
        if self.pending != Some((reply.heartbeat_id(), reply.sequence())) {
            return Err(invalid(
                "heartbeat reply does not match the outstanding nonce and sequence",
            ));
        }
        self.pending = None;
        self.missed = 0;
        Ok(())
    }
}

fn derive_id(context: ProtocolContext, sequence: u64) -> Result<HeartbeatId, DaemonError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/daemon-heartbeat/v1\0");
    hasher.update(context.session_id().as_bytes());
    hasher.update(sequence.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    if bytes == [0; 16] {
        bytes[0] = 1;
    }
    HeartbeatId::new(bytes).map_err(|_| heartbeat_failure())
}

fn heartbeat_failure() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Transport,
        DaemonRecovery::Retry,
        "exchange application heartbeat",
        "application heartbeat reply bound was exceeded",
    )
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate application heartbeat",
        detail,
    )
}
