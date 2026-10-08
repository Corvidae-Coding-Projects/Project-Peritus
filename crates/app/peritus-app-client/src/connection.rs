//! Negotiated local daemon connections with explicitly serialized requests.

use std::{ffi::OsStr, time::Duration};

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, CURRENT_PROTOCOL_RANGE, ClientHello, NegotiationOutcome,
    ProtocolContext, ProtocolFeatureName, ProtocolFeatureSet, ProtocolId, WellKnownProtocolFeature,
};
use peritus_types::SessionId;

use crate::{error::ClientError, frame::FrameStream, identity::nonzero_id};

/// One negotiated connection to the daemon.
///
/// Requests use exclusive mutable access. Subscriptions must use a separate
/// connection so event frames cannot be mistaken for request responses. A failed
/// or cancelled frame exchange invalidates this connection; reconnect and inspect
/// the original durable receipt before considering a mutation retry.
pub struct Client {
    pub(crate) stream: FrameStream,
    pub(crate) context: ProtocolContext,
    pub(crate) timeout: Option<Duration>,
    pub(crate) usable: bool,
    features: ProtocolFeatureSet,
}

impl Client {
    /// Connects and negotiates the required daemon capabilities.
    ///
    /// The timeout covers the entire connection and negotiation exchange.
    ///
    /// # Errors
    /// Returns an error for unavailable transport, timeout, invalid negotiation,
    /// missing required features, or failure to establish the requested session.
    pub async fn connect(
        endpoint: &OsStr,
        requested_session: Option<SessionId>,
        timeout: impl Into<Option<Duration>>,
        required: &[WellKnownProtocolFeature],
    ) -> Result<Self, ClientError> {
        let timeout = timeout.into();
        crate::optional_timeout(
            timeout,
            Self::negotiate(endpoint, requested_session, timeout, required),
        )
        .await
        .map_err(|_| {
            ClientError::connection("connect and negotiate daemon session", "operation timed out")
        })?
    }

    async fn negotiate(
        endpoint: &OsStr,
        requested_session: Option<SessionId>,
        timeout: Option<Duration>,
        required: &[WellKnownProtocolFeature],
    ) -> Result<Self, ClientError> {
        let mut stream = FrameStream::connect(endpoint).await?;
        let protocol_id = ProtocolId::new(nonzero_id()?)?;
        let required_features = required
            .iter()
            .copied()
            .map(ProtocolFeatureName::well_known)
            .collect::<Result<Vec<_>, _>>()?;
        let optional = [
            WellKnownProtocolFeature::TerminalFailure,
            WellKnownProtocolFeature::TerminalOutputGaps,
            WellKnownProtocolFeature::TerminalPipes,
            WellKnownProtocolFeature::WorkbenchInputMoves,
            WellKnownProtocolFeature::WorkbenchRequestSources,
            WellKnownProtocolFeature::WorkbenchFileSources,
            WellKnownProtocolFeature::HarnessImprovements,
            WellKnownProtocolFeature::HarnessImprovementPages,
            WellKnownProtocolFeature::WorkbenchCheckpointCoverage,
            WellKnownProtocolFeature::WorkbenchCheckpointManifests,
        ]
        .into_iter()
        .filter(|feature| !required.contains(feature))
        .map(ProtocolFeatureName::well_known)
        .collect::<Result<Vec<_>, _>>()?;
        let hello = ClientHello::new_with_session(
            protocol_id,
            requested_session,
            vec![CURRENT_PROTOCOL_RANGE],
            required_features,
            optional,
            AppProtocolLimits::PRODUCTION,
            format!("peritus/{}", env!("CARGO_PKG_VERSION")),
        )?;
        stream.write(&AppMessage::ClientHello(hello.clone())).await?;
        let AppMessage::ServerHello(server) = stream.read().await? else {
            return Err(ClientError::negotiation("daemon did not answer with ServerHello"));
        };
        if server.protocol_id() != protocol_id {
            return Err(ClientError::negotiation("daemon echoed a different protocol identity"));
        }
        let (version, limits, features) = match server.outcome() {
            NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol) => {
                if protocol.version() != CURRENT_PROTOCOL_RANGE.preferred()
                    || !hello.accepts(protocol)
                {
                    return Err(ClientError::negotiation(
                        "daemon selected a version, feature, or capacity outside the client offer",
                    ));
                }
                for feature in required {
                    let name = ProtocolFeatureName::well_known(*feature)?;
                    if !protocol.features().contains(&name) {
                        return Err(ClientError::negotiation(format!(
                            "daemon omitted required capability: {}",
                            name.as_str(),
                        )));
                    }
                }
                (protocol.version(), protocol.limits(), protocol.features().clone())
            }
            NegotiationOutcome::Incompatible(reason) => {
                return Err(ClientError::negotiation(format!("incompatible protocol: {reason:?}")));
            }
        };
        let session = server.established_session().ok_or_else(|| {
            ClientError::negotiation("compatible negotiation did not establish a durable session")
        })?;
        if requested_session.is_some_and(|requested| requested != session) {
            return Err(ClientError::negotiation(
                "daemon established a different requested session",
            ));
        }
        stream.set_limits(limits)?;
        Ok(Self {
            stream,
            context: ProtocolContext::new(protocol_id, version, session),
            timeout,
            usable: true,
            features,
        })
    }

    /// Returns the negotiated protocol, version, and durable IPC session identity.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }

    /// Returns the limits negotiated with the daemon.
    #[must_use]
    pub const fn limits(&self) -> AppProtocolLimits {
        self.stream.limits()
    }

    /// Reports whether the peer negotiated a particular application representation.
    #[must_use]
    pub fn supports(&self, feature: WellKnownProtocolFeature) -> bool {
        self.features.as_slice().iter().any(|name| name.as_str() == feature.as_str())
    }

    /// Whether the stream has completed every previous frame exchange.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.usable
    }

    pub(crate) fn begin_exchange(&mut self) -> Result<(), ClientError> {
        if !self.usable {
            return Err(ClientError::connection(
                "use daemon connection",
                "connection requires replacement after an interrupted or failed exchange",
            ));
        }
        // Set this before awaiting so cancellation cannot leave a partial frame
        // readable by a later operation on the same connection.
        self.usable = false;
        Ok(())
    }
}
