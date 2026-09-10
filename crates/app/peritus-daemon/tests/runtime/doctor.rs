//! Actual authenticated IPC rejects unnegotiated doctor requests before host dispatch.

use super::*;
use peritus_app_protocol::{
    AppErrorCode, DoctorQuery, ProtocolFeatureName, WellKnownProtocolFeature,
};

#[test]
fn doctor_requires_negotiation_and_reports_missing_prerequisites_without_starting_work() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(support::configuration(temporary.path())).await.expect("start");
        for enabled in [false, true] {
            let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
            let stream = UnixStream::connect(socket).await.expect("connect");
            let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
            let optional = if enabled {
                vec![
                    ProtocolFeatureName::well_known(WellKnownProtocolFeature::ProductDiagnostics)
                        .expect("feature"),
                ]
            } else {
                Vec::new()
            };
            let client = ClientHello::new(
                ProtocolId::new([if enabled { 42 } else { 41 }; 16]).expect("protocol"),
                vec![VersionRange::new(1, 0, 0).expect("version")],
                Vec::new(),
                optional,
                AppProtocolLimits::PRODUCTION,
                "doctor-test".to_owned(),
            )
            .expect("hello");
            frames.write(&AppMessage::ClientHello(client.clone())).await.expect("hello");
            let AppMessage::ServerHello(server) = frames.read().await.expect("server hello") else {
                panic!("hello")
            };
            let protocol = match server.outcome() {
                NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => {
                    value
                }
                NegotiationOutcome::Incompatible(reason) => panic!("incompatible {reason:?}"),
            };
            let query = DoctorQuery::new(
                peritus_types::WorkspaceId::new([11; 16]).expect("workspace"),
                None,
            );
            let request = AppRequestEnvelope::new(
                ProtocolContext::new(
                    client.protocol_id(),
                    protocol.version(),
                    server.established_session().expect("session"),
                ),
                RequestId::new([12; 16]).expect("request"),
                CorrelationId::new([13; 16]).expect("correlation"),
                AppRequestPayload::Doctor(query),
            )
            .expect("query");
            frames.write(&AppMessage::Request(request.clone())).await.expect("write");
            let AppMessage::Response(response) = frames.read().await.expect("response") else {
                panic!("response")
            };
            assert_eq!(response.request_id(), request.request_id());
            if enabled {
                let AppResponsePayload::Doctor(report) = response.payload() else {
                    panic!("report: {response:?}")
                };
                assert_eq!(report.query(), query);
                assert!(
                    report.findings().iter().any(|finding| finding.check() == "provider-route"
                        && finding.status() == peritus_app_protocol::DoctorStatus::Blocked)
                );
                assert!(
                    report.findings().iter().any(|finding| finding.check() == "workspace-activity"
                        && finding.observation().starts_with("No active"))
                );
            } else {
                let AppResponsePayload::Error(error) = response.payload() else {
                    panic!("expected rejection")
                };
                assert_eq!(error.code(), AppErrorCode::MissingRequiredFeature);
            }
        }
        runtime.shutdown().await.expect("shutdown");
    });
}
