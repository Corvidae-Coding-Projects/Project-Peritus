//! Deterministic, insertion-order-independent protocol negotiation.

use super::{ProtocolFeatureName, ProtocolFeatureSet, ProtocolVersion, VersionRange};
use crate::{AppErrorCode, AppProtocolError, AppProtocolLimits, ProtocolId};
use peritus_types::SessionId;

/// Maximum UTF-8 bytes in one implementation identifier.
pub const MAX_IMPLEMENTATION_METADATA_BYTES: usize = 256;

/// Bounded informational implementation identifier with no compatibility semantics.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImplementationMetadata(String);

impl ImplementationMetadata {
    /// Creates nonempty bounded metadata.
    ///
    /// # Errors
    ///
    /// Returns a malformed-frame error for empty text and a limit error above the smaller of the
    /// negotiated string ceiling and `MAX_IMPLEMENTATION_METADATA_BYTES`.
    pub fn new(value: String, limits: AppProtocolLimits) -> Result<Self, AppProtocolError> {
        if value.is_empty() {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        let ceiling = limits.codec().max_string_bytes.min(MAX_IMPLEMENTATION_METADATA_BYTES);
        if value.len() > ceiling {
            Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None))
        } else {
            Ok(Self(value))
        }
    }

    /// Borrows the exact informational text.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Checked client negotiation input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientHello {
    protocol_id: ProtocolId,
    requested_session: Option<SessionId>,
    versions: Vec<VersionRange>,
    required_features: ProtocolFeatureSet,
    optional_features: ProtocolFeatureSet,
    receive_limits: AppProtocolLimits,
    implementation: ImplementationMetadata,
}

impl ClientHello {
    /// Creates canonical client negotiation input.
    ///
    /// Version ranges are sorted and must be nonoverlapping. Required and optional feature sets
    /// are sorted, duplicate-free, and disjoint.
    ///
    /// # Errors
    ///
    /// Returns a stable application error for empty/overlapping versions, duplicate or overlapping
    /// features, oversized collections, or invalid metadata.
    pub fn new(
        protocol_id: ProtocolId,
        versions: Vec<VersionRange>,
        required_features: Vec<ProtocolFeatureName>,
        optional_features: Vec<ProtocolFeatureName>,
        receive_limits: AppProtocolLimits,
        implementation: String,
    ) -> Result<Self, AppProtocolError> {
        Self::new_with_session(
            protocol_id,
            None,
            versions,
            required_features,
            optional_features,
            receive_limits,
            implementation,
        )
    }

    /// Creates canonical client negotiation input with an optional durable session to resume.
    ///
    /// # Errors
    ///
    /// Returns the same stable validation errors as [`Self::new`].
    pub fn new_with_session(
        protocol_id: ProtocolId,
        requested_session: Option<SessionId>,
        versions: Vec<VersionRange>,
        required_features: Vec<ProtocolFeatureName>,
        optional_features: Vec<ProtocolFeatureName>,
        receive_limits: AppProtocolLimits,
        implementation: String,
    ) -> Result<Self, AppProtocolError> {
        let versions = canonical_versions(versions, receive_limits.max_versions())?;
        let required_features =
            ProtocolFeatureSet::new(required_features, receive_limits.max_features())?;
        let optional_features =
            ProtocolFeatureSet::new(optional_features, receive_limits.max_features())?;
        let total_features = required_features
            .len()
            .checked_add(optional_features.len())
            .ok_or_else(|| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
        if total_features > receive_limits.max_features() {
            return Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None));
        }
        if required_features.as_slice().iter().any(|name| optional_features.contains(name)) {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        let implementation = ImplementationMetadata::new(implementation, receive_limits)?;
        Ok(Self {
            protocol_id,
            requested_session,
            versions,
            required_features,
            optional_features,
            receive_limits,
            implementation,
        })
    }

    /// Returns the relationship identity that the server must echo.
    #[must_use]
    pub const fn protocol_id(&self) -> ProtocolId {
        self.protocol_id
    }
    /// Returns the durable session requested for resumption, if any.
    #[must_use]
    pub const fn requested_session(&self) -> Option<SessionId> {
        self.requested_session
    }
    /// Borrows canonical supported version ranges.
    #[must_use]
    pub const fn versions(&self) -> &[VersionRange] {
        self.versions.as_slice()
    }
    /// Borrows required features.
    #[must_use]
    pub const fn required_features(&self) -> &ProtocolFeatureSet {
        &self.required_features
    }
    /// Borrows optional features.
    #[must_use]
    pub const fn optional_features(&self) -> &ProtocolFeatureSet {
        &self.optional_features
    }
    /// Returns client receive limits.
    #[must_use]
    pub const fn receive_limits(&self) -> AppProtocolLimits {
        self.receive_limits
    }
    /// Borrows informational implementation metadata.
    #[must_use]
    pub const fn implementation(&self) -> &ImplementationMetadata {
        &self.implementation
    }
}

/// Checked local server negotiation capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerCapabilities {
    versions: Vec<VersionRange>,
    features: ProtocolFeatureSet,
    receive_limits: AppProtocolLimits,
    implementation: ImplementationMetadata,
}

impl ServerCapabilities {
    /// Creates canonical server capability input.
    ///
    /// # Errors
    ///
    /// Returns a stable application error for invalid collections or implementation metadata.
    pub fn new(
        versions: Vec<VersionRange>,
        features: Vec<ProtocolFeatureName>,
        receive_limits: AppProtocolLimits,
        implementation: String,
    ) -> Result<Self, AppProtocolError> {
        Ok(Self {
            versions: canonical_versions(versions, receive_limits.max_versions())?,
            features: ProtocolFeatureSet::new(features, receive_limits.max_features())?,
            receive_limits,
            implementation: ImplementationMetadata::new(implementation, receive_limits)?,
        })
    }

    /// Borrows canonical supported version ranges.
    #[must_use]
    pub const fn versions(&self) -> &[VersionRange] {
        self.versions.as_slice()
    }
    /// Borrows supported features.
    #[must_use]
    pub const fn features(&self) -> &ProtocolFeatureSet {
        &self.features
    }
    /// Returns server receive limits.
    #[must_use]
    pub const fn receive_limits(&self) -> AppProtocolLimits {
        self.receive_limits
    }
    /// Borrows informational implementation metadata.
    #[must_use]
    pub const fn implementation(&self) -> &ImplementationMetadata {
        &self.implementation
    }
}

/// Successfully selected protocol contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegotiatedProtocol {
    version: ProtocolVersion,
    features: ProtocolFeatureSet,
    limits: AppProtocolLimits,
}

impl NegotiatedProtocol {
    /// Creates a protocol selection from already checked canonical components.
    #[must_use]
    pub const fn new(
        version: ProtocolVersion,
        features: ProtocolFeatureSet,
        limits: AppProtocolLimits,
    ) -> Self {
        Self { version, features, limits }
    }

    /// Returns the selected greatest mutually supported version.
    #[must_use]
    pub const fn version(&self) -> ProtocolVersion {
        self.version
    }
    /// Borrows the required features plus mutually supported optional features.
    #[must_use]
    pub const fn features(&self) -> &ProtocolFeatureSet {
        &self.features
    }
    /// Returns the pointwise minimum resource ceilings.
    #[must_use]
    pub const fn limits(&self) -> AppProtocolLimits {
        self.limits
    }
}

/// Closed incompatibility reason vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IncompatibilityReason {
    /// The peers share no major/minor version.
    NoCommonVersion,
    /// The server cannot provide these required client features.
    MissingRequiredFeatures(ProtocolFeatureSet),
}

impl IncompatibilityReason {
    /// Returns the permanently assigned version-one reason tag.
    #[must_use]
    pub const fn tag(&self) -> u8 {
        match self {
            Self::NoCommonVersion => 1,
            Self::MissingRequiredFeatures(_) => 2,
        }
    }
}

/// Deterministic negotiation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NegotiationOutcome {
    /// Both peers received their preferred version, all client optionals, and untightened limits.
    Compatible(NegotiatedProtocol),
    /// A usable protocol was selected with an explicit downgrade.
    Downgraded(NegotiatedProtocol),
    /// No usable protocol can satisfy the client requirements.
    Incompatible(IncompatibilityReason),
}

/// Server negotiation output that echoes the client relationship identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerHello {
    protocol_id: ProtocolId,
    implementation: ImplementationMetadata,
    established_session: Option<SessionId>,
    outcome: NegotiationOutcome,
}

impl ServerHello {
    /// Creates a server hello with session presence matching the negotiation outcome.
    ///
    /// # Errors
    ///
    /// Returns a malformed-frame error when a compatible outcome lacks a session or an
    /// incompatible outcome claims one.
    pub fn new(
        protocol_id: ProtocolId,
        implementation: ImplementationMetadata,
        established_session: Option<SessionId>,
        outcome: NegotiationOutcome,
    ) -> Result<Self, AppProtocolError> {
        let compatible = matches!(
            outcome,
            NegotiationOutcome::Compatible(_) | NegotiationOutcome::Downgraded(_)
        );
        if compatible != established_session.is_some() {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { protocol_id, implementation, established_session, outcome })
    }

    /// Returns the echoed client relationship identity.
    #[must_use]
    pub const fn protocol_id(&self) -> ProtocolId {
        self.protocol_id
    }
    /// Borrows server implementation metadata.
    #[must_use]
    pub const fn implementation(&self) -> &ImplementationMetadata {
        &self.implementation
    }
    /// Returns the established durable session for a usable negotiation.
    #[must_use]
    pub const fn established_session(&self) -> Option<SessionId> {
        self.established_session
    }
    /// Borrows the deterministic outcome.
    #[must_use]
    pub const fn outcome(&self) -> &NegotiationOutcome {
        &self.outcome
    }
}

mod selection;

/// Selects the greatest common version and canonical features, independent of insertion order.
///
/// # Errors
///
/// Returns [`AppErrorCode::InvalidLimits`] only if pointwise intersection exposes an invalid
/// limit configuration, which cannot occur for values built through [`AppProtocolLimits`].
pub fn negotiate(
    client: &ClientHello,
    server: &ServerCapabilities,
    established_session: SessionId,
) -> Result<ServerHello, AppProtocolError> {
    selection::select(client, server, established_session)
}

fn canonical_versions(
    mut versions: Vec<VersionRange>,
    maximum: usize,
) -> Result<Vec<VersionRange>, AppProtocolError> {
    if versions.is_empty() {
        return Err(AppProtocolError::new(AppErrorCode::InvalidVersion, None));
    }
    if versions.len() > maximum {
        return Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None));
    }
    versions.sort_unstable();
    if versions.windows(2).any(|pair| {
        pair[0].major() == pair[1].major() && pair[1].minor_min() <= pair[0].minor_max()
    }) {
        return Err(AppProtocolError::new(AppErrorCode::InvalidVersion, None));
    }
    Ok(versions)
}

#[cfg(test)]
#[path = "negotiation/tests.rs"]
mod tests;
