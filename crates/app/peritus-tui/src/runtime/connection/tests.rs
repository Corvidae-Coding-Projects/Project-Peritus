use super::*;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncReadExt as _;

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn stall(connection: &mut Connection) -> Arc<AtomicBool> {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    connection.attempt = Some(Box::pin(async move {
        let _signal = signal;
        std::future::pending().await
    }));
    dropped
}

#[tokio::test]
async fn stalled_handshake_still_processes_navigation_and_quit() {
    let (events, mut event_rx) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let dropped = stall(&mut connection);
    let (input, mut input_rx) = mpsc::channel(2);
    input.send(Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))).await.unwrap();
    input.send(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL))).await.unwrap();
    let mut model = AppModel::new([81; 32]);
    let mut tick = tokio::time::interval(super::super::UI_TICK);
    let mut reads = super::super::LocalReads::default();
    tokio::time::timeout(std::time::Duration::from_millis(250), async {
        loop {
            let effects = super::super::next_effects(
                &mut model,
                &mut input_rx,
                &mut event_rx,
                &mut tick,
                &mut reads,
                &mut connection,
            )
            .await
            .unwrap();
            if effects.iter().any(|effect| matches!(effect, Effect::Quit)) {
                break;
            }
        }
    })
    .await
    .expect("UI input must not wait for the handshake timeout");
    assert_eq!(model.view, crate::model::View::Help);
    assert!(connection.active());
    connection.cancel();
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn repeated_reconnect_cancels_old_attempt_and_preserves_unsent_draft() {
    let (events, _receiver) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let dropped = stall(&mut connection);
    let mut model = AppModel::new([82; 32]);
    model.chat.buffer = "my unsent draft".into();
    connection.start(&TuiConfig::new("/unused/test-endpoint"), &mut model);
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(model.chat.buffer, "my unsent draft");
    assert!(connection.active());
    assert_eq!(connection.generation, 1);
    assert!(model.protocol_context().is_none());
    assert!(matches!(model.connection, crate::model::ConnectionStatus::Connecting));
}

#[test]
fn dropping_runtime_connection_drops_pending_handshake() {
    let (events, _receiver) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let dropped = stall(&mut connection);
    drop(connection);
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stalled_writer_still_processes_quit_and_sends_in_order_when_drained() {
    use peritus_app_protocol::{
        AppMessage, AppProtocolLimits, ClientHello, ProtocolId, VersionRange,
    };
    let (events, mut event_rx) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let (session, mut peer) = ClientSession::test_pair();
    connection.session = Some(session);
    let mut expected = Vec::new();
    for byte in 1..=16 {
        let hello = ClientHello::new(
            ProtocolId::new([byte; 16]).unwrap(),
            vec![VersionRange::new(1, 0, 0).unwrap()],
            Vec::new(),
            Vec::new(),
            AppProtocolLimits::PRODUCTION,
            "outbox test".into(),
        )
        .unwrap();
        let message = AppMessage::ClientHello(hello);
        expected.push(message.clone());
        connection.send(message).unwrap();
    }
    let (input, mut input_rx) = mpsc::channel(1);
    let mut model = AppModel::new([83; 32]);
    let mut tick = tokio::time::interval(super::super::UI_TICK);
    let mut reads = super::super::LocalReads::default();
    // The 64-byte peer is deliberately unread, so writer admission eventually stalls.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(25), async {
        while connection.sending() {
            super::super::next_effects(
                &mut model,
                &mut input_rx,
                &mut event_rx,
                &mut tick,
                &mut reads,
                &mut connection,
            )
            .await
            .unwrap();
        }
    })
    .await;
    assert!(connection.sending(), "the fixture must actually fill its writer queue");
    input.send(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL))).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_millis(250), async {
        loop {
            let effects = super::super::next_effects(
                &mut model,
                &mut input_rx,
                &mut event_rx,
                &mut tick,
                &mut reads,
                &mut connection,
            )
            .await
            .unwrap();
            if effects.iter().any(|effect| matches!(effect, Effect::Quit)) {
                break;
            }
        }
    })
    .await
    .expect("quit must remain responsive under actual writer backpressure");
    let read = async {
        let mut bytes = Vec::new();
        peer.read_to_end(&mut bytes).await.unwrap();
        bytes
    };
    let (closed, actual) = tokio::join!(connection.close(Vec::new()), read);
    closed.unwrap();
    let expected = expected
        .into_iter()
        .flat_map(|message| {
            peritus_app_protocol::encode_app_message(&message, AppProtocolLimits::PRODUCTION)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "orderly exit must drain every queued frame in exact order");
}
