//! Durable peer-bound A3 negotiation.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use peritus_app_protocol::{
    AppProtocolLimits, ClientHello, NegotiatedProtocol, NegotiationOutcome, ProtocolContext,
    ProtocolFeatureName, ServerCapabilities, ServerHello, VersionRange, WellKnownProtocolFeature,
    negotiate,
};
use peritus_journal::{ApplicationPrincipalState, ApplicationSessionState, NewApplicationSession};
use peritus_types::{ActorId, SessionId};
use sha2::{Digest, Sha256};

use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery, PeerIdentity};

static SESSION_NONCE: AtomicU64 = AtomicU64::new(1);

/// Exact authenticated and negotiated relationship used by every post-hello frame.
#[derive(Clone, Debug)]
pub struct ConnectionContext {
    actor_id: ActorId,
    protocol: ProtocolContext,
    negotiated: NegotiatedProtocol,
}

impl ConnectionContext {
    /// Returns the authenticated durable human actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the exact context required on every subsequent frame.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolContext {
        self.protocol
    }
    /// Returns the negotiated receive and send limits.
    #[must_use]
    pub const fn limits(&self) -> AppProtocolLimits {
        self.negotiated.limits()
    }
}

pub(crate) struct Establishment {
    pub(crate) hello: ServerHello,
    pub(crate) context: Option<ConnectionContext>,
}

pub(crate) async fn establish(
    authority: &AuthorityHandle,
    peer: PeerIdentity,
    client: &ClientHello,
) -> Result<Establishment, DaemonError> {
    let principal = authority.principal(peer.principal_digest()).await?.ok_or_else(|| {
        unauthorized("authenticated operating-system principal is not provisioned")
    })?;
    if principal.kind() != peer.kind() || principal.state() != ApplicationPrincipalState::Active {
        return Err(unauthorized("authenticated operating-system principal binding is inactive"));
    }

    let requested = match client.requested_session() {
        Some(session_id) => {
            let session = authority
                .session(session_id)
                .await?
                .ok_or_else(|| unauthorized("requested durable session does not exist"))?;
            if session.actor_id() != principal.actor_id()
                || session.state() != ApplicationSessionState::Active
            {
                return Err(unauthorized(
                    "requested durable session is not active for the authenticated actor",
                ));
            }
            Some(session_id)
        }
        None => None,
    };
    let candidate = requested.unwrap_or_else(|| new_session_id(peer, client));
    let capabilities = server_capabilities()?;
    let hello = negotiate(client, &capabilities, candidate).map_err(protocol_error)?;
    let negotiated = match hello.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => {
            value.clone()
        }
        NegotiationOutcome::Incompatible(_) => {
            return Ok(Establishment { hello, context: None });
        }
    };

    let epoch = authority.authority_epoch().await?.ok_or_else(|| {
        DaemonError::new(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::Operator,
            "establish application session",
            "journal has no current authority epoch",
        )
    })?;
    if requested.is_some() {
        authority
            .observe_session(
                candidate,
                principal.actor_id(),
                client.protocol_id().into_bytes(),
                negotiated.version().major(),
                negotiated.version().minor(),
            )
            .await?;
    } else {
        let session = NewApplicationSession::new(
            candidate,
            principal.actor_id(),
            epoch,
            creation_tick(),
            client.protocol_id().into_bytes(),
            negotiated.version().major(),
            negotiated.version().minor(),
        )
        .map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "construct application session",
                error.to_string(),
                error,
            )
        })?;
        authority.open_session(session).await?;
    }

    let protocol = ProtocolContext::new(client.protocol_id(), negotiated.version(), candidate);
    Ok(Establishment {
        hello,
        context: Some(ConnectionContext { actor_id: principal.actor_id(), protocol, negotiated }),
    })
}

fn server_capabilities() -> Result<ServerCapabilities, DaemonError> {
    let features = [
        WellKnownProtocolFeature::EventSubscriptions,
        WellKnownProtocolFeature::ArtifactTransfer,
        WellKnownProtocolFeature::ApprovalPrompts,
        WellKnownProtocolFeature::UserInput,
        WellKnownProtocolFeature::TerminalStreaming,
        WellKnownProtocolFeature::ReadOnlyDiagnostics,
        WellKnownProtocolFeature::GracefulShutdown,
    ]
    .into_iter()
    .map(ProtocolFeatureName::well_known)
    .collect::<Result<Vec<_>, _>>()
    .map_err(protocol_error)?;
    ServerCapabilities::new(
        vec![VersionRange::new(1, 0, 0).map_err(protocol_error)?],
        features,
        AppProtocolLimits::PRODUCTION,
        format!("peritusd/{}", env!("CARGO_PKG_VERSION")),
    )
    .map_err(protocol_error)
}

fn new_session_id(peer: PeerIdentity, client: &ClientHello) -> SessionId {
    let nonce = SESSION_NONCE.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/application-session/v1\0");
    hasher.update(peer.principal_digest().as_bytes());
    hasher.update(client.protocol_id().as_bytes());
    hasher.update(std::process::id().to_be_bytes());
    hasher.update(creation_tick().to_be_bytes());
    hasher.update(nonce.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    SessionId::new(bytes).expect("SHA-256 prefix plus nonzero domain input is nonzero")
}

fn creation_tick() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
    millis.max(1)
}

fn protocol_error(error: peritus_app_protocol::AppProtocolError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "negotiate application protocol",
        error.to_string(),
        error,
    )
}

fn unauthorized(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Unauthorized,
        DaemonRecovery::CorrectRequest,
        "establish application session",
        detail,
    )
}
