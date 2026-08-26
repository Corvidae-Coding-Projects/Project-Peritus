//! Deterministic, insertion-order-independent protocol negotiation.

use super::{ProtocolFeatureName, ProtocolFeatureSet, ProtocolVersion, VersionRange};
use crate::{AppErrorCode, AppProtocolError, AppProtocolLimits, ProtocolId};

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
    outcome: NegotiationOutcome,
}

impl ServerHello {
    /// Creates a server hello from already checked negotiation components.
    #[must_use]
    pub const fn new(
        protocol_id: ProtocolId,
        implementation: ImplementationMetadata,
        outcome: NegotiationOutcome,
    ) -> Self {
        Self { protocol_id, implementation, outcome }
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
    /// Borrows the deterministic outcome.
    #[must_use]
    pub const fn outcome(&self) -> &NegotiationOutcome {
        &self.outcome
    }
}

/// Selects the greatest common version and canonical features, independent of insertion order.
///
/// # Errors
///
/// Returns [`AppErrorCode::InvalidLimits`] only if pointwise intersection exposes an invalid
/// limit configuration, which cannot occur for values built through [`AppProtocolLimits`].
pub fn negotiate(
    client: &ClientHello,
    server: &ServerCapabilities,
) -> Result<ServerHello, AppProtocolError> {
    let Some(version) = greatest_common_version(client.versions(), server.versions()) else {
        return Ok(server_hello(
            client,
            server,
            NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
        ));
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
        return Ok(server_hello(
            client,
            server,
            NegotiationOutcome::Incompatible(IncompatibilityReason::MissingRequiredFeatures(
                missing,
            )),
        ));
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
    Ok(server_hello(client, server, outcome))
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
    outcome: NegotiationOutcome,
) -> ServerHello {
    ServerHello {
        protocol_id: client.protocol_id(),
        implementation: server.implementation.clone(),
        outcome,
    }
}

#[cfg(test)]
#[path = "negotiation/tests.rs"]
mod tests;
