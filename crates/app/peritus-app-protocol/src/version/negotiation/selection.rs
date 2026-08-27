//! Greatest-common-version and feature selection.

use super::{
    ClientHello, IncompatibilityReason, NegotiatedProtocol, NegotiationOutcome, ServerCapabilities,
    ServerHello,
};
use crate::{AppErrorCode, AppProtocolError, ProtocolFeatureSet, ProtocolVersion, VersionRange};
use peritus_types::SessionId;

/// Selects the greatest common version and canonical features, independent of insertion order.
///
/// # Errors
///
/// Returns [`AppErrorCode::InvalidLimits`] only if pointwise intersection exposes an invalid
/// limit configuration, which cannot occur for values built through [`crate::AppProtocolLimits`].
pub(super) fn select(
    client: &ClientHello,
    server: &ServerCapabilities,
    established_session: SessionId,
) -> Result<ServerHello, AppProtocolError> {
    let Some(version) = greatest_common_version(client.versions(), server.versions()) else {
        return server_hello(
            client,
            server,
            None,
            NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
        );
    };
    let missing = client
        .required_features()
        .as_slice()
        .iter()
        .filter(|feature| !server.features().contains(feature))
        .cloned()
        .collect();
    let missing = ProtocolFeatureSet::new(missing, client.receive_limits().max_features())?;
    if !missing.is_empty() {
        return server_hello(
            client,
            server,
            None,
            NegotiationOutcome::Incompatible(IncompatibilityReason::MissingRequiredFeatures(
                missing,
            )),
        );
    }

    let selected_optional = client.optional_features().intersection(server.features());
    let mut selected = client.required_features().as_slice().to_vec();
    selected.extend(selected_optional.as_slice().iter().cloned());
    let features = ProtocolFeatureSet::new(selected, client.receive_limits().max_features())?;
    let limits = client
        .receive_limits()
        .negotiated(server.receive_limits())
        .map_err(|_| AppProtocolError::new(AppErrorCode::InvalidLimits, None))?;
    let protocol = NegotiatedProtocol { version, features, limits };
    let compatible = Some(version) == preferred(client.versions())
        && Some(version) == preferred(server.versions())
        && selected_optional.len() == client.optional_features().len()
        && server.receive_limits().permits_all(client.receive_limits());
    let outcome = if compatible {
        NegotiationOutcome::Compatible(protocol)
    } else {
        NegotiationOutcome::Downgraded(protocol)
    };
    server_hello(client, server, Some(established_session), outcome)
}

fn preferred(ranges: &[VersionRange]) -> Option<ProtocolVersion> {
    ranges.last().copied().map(VersionRange::preferred)
}

fn greatest_common_version(
    client: &[VersionRange],
    server: &[VersionRange],
) -> Option<ProtocolVersion> {
    client
        .iter()
        .flat_map(|left| server.iter().filter_map(|right| left.greatest_intersection(*right)))
        .max()
}

fn server_hello(
    client: &ClientHello,
    server: &ServerCapabilities,
    established_session: Option<SessionId>,
    outcome: NegotiationOutcome,
) -> Result<ServerHello, AppProtocolError> {
    ServerHello::new(
        client.protocol_id(),
        server.implementation.clone(),
        established_session,
        outcome,
    )
}
