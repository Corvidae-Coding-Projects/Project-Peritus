use super::*;

#[test]
fn negotiation_selects_greatest_common_version_and_marks_downgrade() {
    let protocol_id = ProtocolId::new([1; 16]).unwrap();
    let optional = ProtocolFeatureName::well_known(
        super::super::super::WellKnownProtocolFeature::GracefulShutdown,
    )
    .unwrap();
    let client = ClientHello::new(
        protocol_id,
        vec![VersionRange::new(1, 0, 3).unwrap()],
        Vec::new(),
        vec![optional.clone()],
        AppProtocolLimits::PRODUCTION,
        "test-client".to_owned(),
    )
    .unwrap();
    let server = ServerCapabilities::new(
        vec![VersionRange::new(1, 0, 2).unwrap()],
        vec![optional],
        AppProtocolLimits::PRODUCTION,
        "test-server".to_owned(),
    )
    .unwrap();
    let hello = negotiate(&client, &server).unwrap();
    match hello.outcome() {
        NegotiationOutcome::Downgraded(protocol) => {
            assert_eq!(protocol.version(), ProtocolVersion::new(1, 2).unwrap());
        }
        other => panic!("expected downgrade, received {other:?}"),
    }
}
