//! Authenticated A3 session establishment and connection ownership.

use peritus_app_protocol::{AppEventPayload, ShutdownRequest};
use tokio::sync::mpsc;

mod connection;
mod heartbeat;
mod negotiation;
mod request;

pub use connection::run_connection;
pub use negotiation::ConnectionContext;

const SHUTDOWN_EVENT_CAPACITY: usize = 8;

pub type ShutdownEventReceiver = mpsc::Receiver<AppEventPayload>;

pub struct ShutdownCommand {
    request: ShutdownRequest,
    events: mpsc::Sender<AppEventPayload>,
}

impl ShutdownCommand {
    pub(super) fn new(request: ShutdownRequest) -> (Self, ShutdownEventReceiver) {
        let (events, receiver) = mpsc::channel(SHUTDOWN_EVENT_CAPACITY);
        (Self { request, events }, receiver)
    }

    pub(super) const fn request(&self) -> ShutdownRequest {
        self.request
    }

    pub(super) async fn deliver(&self, event: AppEventPayload) {
        let _ = self.events.send(event).await;
    }
}
