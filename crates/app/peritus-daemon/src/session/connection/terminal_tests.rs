//! Attachment delivery failure must not take unrelated application traffic down.

use super::*;
use peritus_app_protocol::{
    ProtocolContext, ProtocolId, ProtocolVersion, RequestId, TerminalAttachmentId, TerminalBinding,
};
use peritus_types::{ActorId, ProcessId, SessionId};

mod native;

#[tokio::test]
async fn unavailable_attachment_does_not_end_the_application_connection() {
    let actor = ActorId::new([1; 16]).unwrap();
    let session = SessionId::new([2; 16]).unwrap();
    let context = ProtocolContext::new(
        ProtocolId::new([3; 16]).unwrap(),
        ProtocolVersion::new(1, 0).unwrap(),
        session,
    );
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([4; 16]).unwrap(),
        ProcessId::new([5; 16]).unwrap(),
        RequestId::new([6; 16]).unwrap(),
    );
    let registry =
        TerminalRegistry::new(crate::terminal::TerminalRegistryLimits::PRODUCTION).unwrap();
    for negotiated in [false, true] {
        let (stream, peer) = tokio::io::duplex(4096);
        let mut frames = crate::AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
        let mut peer = crate::AppFrameStream::new(peer, AppProtocolLimits::PRODUCTION);
        let other = TerminalBinding::new(
            TerminalAttachmentId::new([7; 16]).unwrap(),
            ProcessId::new([8; 16]).unwrap(),
            RequestId::new([9; 16]).unwrap(),
        );
        let mut bindings = vec![binding, other];
        pump_terminals(
            &mut frames,
            &registry,
            &mut bindings,
            actor,
            session,
            context,
            AppProtocolLimits::PRODUCTION.max_diagnostic_bytes(),
            negotiated,
            negotiated,
        )
        .await
        .expect("attachment failure is local");
        assert!(bindings.is_empty(), "failed attachment must not be polled forever");
        for expected in [binding, other] {
            let AppMessage::Event(event) = peer.read().await.unwrap() else {
                panic!("failure event")
            };
            if negotiated {
                assert_eq!(event.payload(), &AppEventPayload::TerminalUnavailable(expected));
            } else {
                assert!(
                    matches!(event.payload(), AppEventPayload::Diagnostic(_)),
                    "old clients must not receive a new event tag"
                );
            }
        }
        let request = AppMessage::Request(
            AppRequestEnvelope::new(
                context,
                RequestId::new([10; 16]).unwrap(),
                peritus_app_protocol::CorrelationId::new([11; 16]).unwrap(),
                peritus_app_protocol::AppRequestPayload::DaemonStatus,
            )
            .unwrap(),
        );
        peer.write(&request).await.unwrap();
        assert_eq!(frames.read().await.unwrap(), request, "unrelated traffic remains usable");
    }
}
