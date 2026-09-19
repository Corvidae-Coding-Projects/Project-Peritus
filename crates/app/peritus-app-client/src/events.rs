//! Event and control exchanges for dedicated subscription connections.

use peritus_app_protocol::{
    AppEventEnvelope, AppEventPayload, AppMessage, ControlEnvelope, ControlPayload, CorrelationId,
    HeartbeatReply,
};

use crate::{ClientError, connection::Client, identity::nonzero_id};

impl Client {
    /// Reads one event from a dedicated subscription connection.
    ///
    /// This read deliberately has no request deadline: an idle subscription may
    /// remain open. Cancelling the future invalidates the stream because a frame
    /// may have been partially read. The caller owns idle detection and reconnect.
    /// Heartbeats are returned so consumers can observe liveness and reply using
    /// [`Self::reply_heartbeat`].
    ///
    /// # Errors
    /// Returns an error if the peer disconnects, changes context, or sends a
    /// non-event frame.
    pub async fn read_event(&mut self) -> Result<AppEventEnvelope, ClientError> {
        self.begin_exchange()?;
        let event = match self.stream.read().await? {
            AppMessage::Event(event) if event.context() == self.context => event,
            AppMessage::Event(_) => {
                return Err(ClientError::protocol(
                    "read daemon event",
                    "event context differs from negotiated context",
                ));
            }
            _ => {
                return Err(ClientError::protocol(
                    "read daemon event",
                    "unexpected frame family while streaming events",
                ));
            }
        };
        self.usable = true;
        Ok(event)
    }

    /// Writes a control frame within the configured request timeout.
    ///
    /// # Errors
    /// Returns an error if writing fails, times out, or the connection was
    /// invalidated by an earlier interrupted exchange.
    pub async fn write_control(
        &mut self,
        correlation_id: CorrelationId,
        payload: ControlPayload,
    ) -> Result<(), ClientError> {
        let control = ControlEnvelope::new(self.context, correlation_id, payload);
        self.begin_exchange()?;
        tokio::time::timeout(self.timeout, self.stream.write(&AppMessage::Control(control)))
            .await
            .map_err(|_| {
                ClientError::connection("write daemon control", "control write timed out")
            })??;
        self.usable = true;
        Ok(())
    }

    /// Replies to a heartbeat belonging to this negotiated connection.
    ///
    /// Returns `false` for an ordinary event without writing a control frame.
    ///
    /// # Errors
    /// Returns an error for foreign event contexts, failed writes, identity
    /// generation failures, timeouts, or an invalidated connection.
    pub async fn reply_heartbeat(&mut self, event: &AppEventEnvelope) -> Result<bool, ClientError> {
        self.begin_exchange()?;
        let replied = tokio::time::timeout(self.timeout, self.reply_heartbeat_inner(event))
            .await
            .map_err(|_| {
                ClientError::connection("reply to daemon heartbeat", "heartbeat reply timed out")
            })??;
        self.usable = true;
        Ok(replied)
    }

    pub(crate) async fn reply_heartbeat_inner(
        &mut self,
        event: &AppEventEnvelope,
    ) -> Result<bool, ClientError> {
        if event.context() != self.context {
            return Err(ClientError::protocol(
                "validate daemon event",
                "event context differs from negotiated context",
            ));
        }
        let AppEventPayload::Heartbeat(heartbeat) = event.payload() else {
            return Ok(false);
        };
        let control = ControlEnvelope::new(
            self.context,
            CorrelationId::new(nonzero_id()?)?,
            ControlPayload::HeartbeatReply(HeartbeatReply::new(
                heartbeat.heartbeat_id(),
                heartbeat.sequence(),
            )),
        );
        self.stream.write(&AppMessage::Control(control)).await?;
        Ok(true)
    }
}
