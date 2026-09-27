use super::*;

const COMMAND_FEATURES: [WellKnownProtocolFeature; 3] = [
    WellKnownProtocolFeature::ConversationLibrary,
    WellKnownProtocolFeature::ConversationForks,
    WellKnownProtocolFeature::WorkbenchExecution,
];

fn command_features() -> Vec<ProtocolFeatureName> {
    COMMAND_FEATURES
        .into_iter()
        .map(|feature| ProtocolFeatureName::well_known(feature).expect("well-known feature"))
        .collect()
}

#[test]
fn client_hello_advertises_features_used_by_implemented_commands() {
    let hello = client_hello(
        ProtocolId::new([1; 16]).expect("protocol"),
        None,
        AppProtocolLimits::PRODUCTION,
    )
    .expect("client hello");

    for feature in command_features() {
        assert!(hello.optional_features().contains(&feature), "missing {feature:?}");
    }
}

#[test]
fn negotiation_selects_each_implemented_command_feature_when_the_daemon_advertises_it() {
    let protocol = ProtocolId::new([2; 16]).expect("protocol");
    let client = client_hello(protocol, None, AppProtocolLimits::PRODUCTION).expect("client hello");
    let server = peritus_app_protocol::ServerCapabilities::new(
        vec![VersionRange::new(1, 0, 0).expect("version")],
        command_features(),
        AppProtocolLimits::PRODUCTION,
        "test-daemon".to_owned(),
    )
    .expect("server capabilities");
    let hello = peritus_app_protocol::negotiate(
        &client,
        &server,
        SessionId::new([3; 16]).expect("session"),
    )
    .expect("negotiation");
    let selected = match hello.outcome() {
        NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol) => {
            protocol.features()
        }
        NegotiationOutcome::Incompatible(reason) => panic!("incompatible: {reason:?}"),
    };

    for feature in command_features() {
        assert!(selected.contains(&feature), "feature was not negotiated: {feature:?}");
    }
}
