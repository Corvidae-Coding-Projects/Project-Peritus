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
