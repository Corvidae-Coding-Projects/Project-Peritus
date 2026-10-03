//! Cancellation-safe receive selection keeps provider reads independent of client traffic.

use super::{CatalogRequests, ConnectionAction};
use std::{
    future::{Future, poll_fn},
    task::Poll,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::watch,
    time::Interval,
};

pub(super) async fn next_action<S: AsyncRead + AsyncWrite + Unpin>(
    frames: &mut crate::AppFrameStream<S>,
    stop: &mut watch::Receiver<bool>,
    delivery_tick: &mut Interval,
    heartbeat_tick: &mut Interval,
    catalogs: &mut CatalogRequests,
) -> ConnectionAction {
    let mut changed = Box::pin(stop.changed());
    let mut message = Box::pin(frames.read_or_eof());
    let mut delivery = Box::pin(delivery_tick.tick());
    let mut heartbeat = Box::pin(heartbeat_tick.tick());
    poll_fn(|context| {
        if let Poll::Ready(changed) = changed.as_mut().poll(context) {
            return Poll::Ready(ConnectionAction::Stop(changed));
        }
        if let Poll::Ready(message) = message.as_mut().poll(context) {
            return Poll::Ready(ConnectionAction::Message(message));
        }
        if let Poll::Ready(response) = catalogs.poll(context) {
            return Poll::Ready(ConnectionAction::Catalog(response));
        }
        if delivery.as_mut().poll(context).is_ready() {
            return Poll::Ready(ConnectionAction::Delivery);
        }
        if heartbeat.as_mut().poll(context).is_ready() {
            return Poll::Ready(ConnectionAction::Heartbeat);
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
        AppResponsePayload, CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
        encode_app_message,
    };
    use std::time::Duration;
    use tokio::io::AsyncWriteExt as _;

    #[tokio::test]
    async fn delayed_catalog_does_not_block_heartbeat_or_a_fragmented_followup_request() {
        let context = ProtocolContext::new(
            ProtocolId::new([1; 16]).unwrap(),
            ProtocolVersion::new(1, 0).unwrap(),
            peritus_types::SessionId::new([2; 16]).unwrap(),
        );
        let request = AppRequestEnvelope::new(
            context,
            RequestId::new([3; 16]).unwrap(),
            CorrelationId::new([4; 16]).unwrap(),
            AppRequestPayload::DaemonStatus,
        )
        .unwrap();
        let message = AppMessage::Request(request.clone());
        let encoded = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
        let (mut peer, stream) = tokio::io::duplex(4096);
        let mut frames = crate::AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
        let (_stop_sender, mut stop) = watch::channel(false);
        let now = tokio::time::Instant::now();
        let mut delivery =
            tokio::time::interval_at(now + Duration::from_hours(1), Duration::from_hours(1));
        let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
        let mut catalogs = CatalogRequests::default();
        let (release, resume) = tokio::sync::oneshot::channel();
        assert!(catalogs.try_spawn(async move {
            resume.await.unwrap();
            AppResponseEnvelope::new(
                context,
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
                    peritus_app_protocol::AppErrorCode::NotReady,
                    None,
                )),
            )
        }));
        peer.write_all(&encoded[..1]).await.unwrap();
        assert!(matches!(
            next_action(&mut frames, &mut stop, &mut delivery, &mut heartbeat, &mut catalogs).await,
            ConnectionAction::Heartbeat
        ));
        peer.write_all(&encoded[1..]).await.unwrap();
        let action = tokio::time::timeout(
            Duration::from_secs(1),
            next_action(&mut frames, &mut stop, &mut delivery, &mut heartbeat, &mut catalogs),
        )
        .await
        .unwrap();
        assert!(
            matches!(action, ConnectionAction::Message(Ok(Some(received))) if received == message)
        );
        release.send(()).unwrap();
        let action = tokio::time::timeout(
            Duration::from_secs(1),
            next_action(&mut frames, &mut stop, &mut delivery, &mut heartbeat, &mut catalogs),
        )
        .await
        .unwrap();
        assert!(matches!(action, ConnectionAction::Catalog(Ok(_))));
        catalogs.shutdown().await;
    }
}
