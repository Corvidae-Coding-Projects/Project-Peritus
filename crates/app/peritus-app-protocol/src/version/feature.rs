//! Canonically ordered protocol feature names.

use crate::{AppErrorCode, AppProtocolError};
use peritus_types::CapabilityName;

/// Validated feature name with no implied authority semantics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolFeatureName(CapabilityName);

impl ProtocolFeatureName {
    /// Version-one event subscription feature name.
    pub const EVENT_SUBSCRIPTIONS: &'static str = "app.event-subscriptions";
    /// Version-one artifact transfer feature name.
    pub const ARTIFACT_TRANSFER: &'static str = "app.artifact-transfer";
    /// Version-one approval prompt feature name.
    pub const APPROVAL_PROMPTS: &'static str = "app.approval-prompts";
    /// Version-one user input feature name.
    pub const USER_INPUT: &'static str = "app.user-input";
    /// Version-one terminal streaming feature name.
    pub const TERMINAL_STREAMING: &'static str = "app.terminal-streaming";
    /// Version-one read-only diagnostics feature name.
    pub const READ_ONLY_DIAGNOSTICS: &'static str = "app.read-only-diagnostics";
    /// Version-one graceful shutdown feature name.
    pub const GRACEFUL_SHUTDOWN: &'static str = "app.graceful-shutdown";

    /// Creates a feature name using the foundation capability-name grammar.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::MalformedFrame`] when the name is not canonical.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        CapabilityName::new(value)
            .map(Self)
            .map_err(|_| AppProtocolError::new(AppErrorCode::MalformedFrame, None))
    }

    /// Creates one closed, well-known version-one feature name.
    ///
    /// # Errors
    ///
    /// Returns an error only if a compiled-in feature name violates the foundation grammar.
    pub fn well_known(feature: WellKnownProtocolFeature) -> Result<Self, AppProtocolError> {
        Self::new(feature.as_str().to_owned())
    }

    /// Borrows the exact canonical feature name.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Compares names by their exact canonical ASCII bytes.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.canonical_cmp(&other.0)
    }
}

/// Closed list of well-known application-protocol version-one features.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WellKnownProtocolFeature {
    /// Replayable event subscriptions.
    EventSubscriptions,
    /// Chunked artifact transfer.
    ArtifactTransfer,
    /// Approval prompt delivery and answer submission.
    ApprovalPrompts,
    /// User-input prompt delivery and answer submission.
    UserInput,
    /// Attached terminal input and output streaming.
    TerminalStreaming,
    /// Read-only diagnostics.
    ReadOnlyDiagnostics,
    /// Graceful daemon shutdown controls.
    GracefulShutdown,
}

impl WellKnownProtocolFeature {
    /// Returns the stable canonical feature name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventSubscriptions => ProtocolFeatureName::EVENT_SUBSCRIPTIONS,
            Self::ArtifactTransfer => ProtocolFeatureName::ARTIFACT_TRANSFER,
            Self::ApprovalPrompts => ProtocolFeatureName::APPROVAL_PROMPTS,
            Self::UserInput => ProtocolFeatureName::USER_INPUT,
            Self::TerminalStreaming => ProtocolFeatureName::TERMINAL_STREAMING,
            Self::ReadOnlyDiagnostics => ProtocolFeatureName::READ_ONLY_DIAGNOSTICS,
            Self::GracefulShutdown => ProtocolFeatureName::GRACEFUL_SHUTDOWN,
        }
    }
}

/// Bounded, sorted, duplicate-free protocol feature collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolFeatureSet(Vec<ProtocolFeatureName>);

impl ProtocolFeatureSet {
    /// Canonicalizes a feature collection under an explicit item ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::LimitExceeded`] when the collection is too large and
    /// [`AppErrorCode::MalformedFrame`] when it contains a duplicate.
    pub fn new(
        mut features: Vec<ProtocolFeatureName>,
        max_features: usize,
    ) -> Result<Self, AppProtocolError> {
        if features.len() > max_features {
            return Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None));
        }
        features.sort_by(ProtocolFeatureName::canonical_cmp);
        if features.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self(features))
    }

    /// Returns an empty canonical feature set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Borrows the canonical sorted names.
    #[must_use]
    pub const fn as_slice(&self) -> &[ProtocolFeatureName] {
        self.0.as_slice()
    }
    /// Returns the number of names.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }
    /// Returns whether the set is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Returns whether the exact feature is present.
    #[must_use]
    pub fn contains(&self, feature: &ProtocolFeatureName) -> bool {
        self.0.binary_search_by(|candidate| candidate.canonical_cmp(feature)).is_ok()
    }
    /// Returns whether every feature in this set occurs in `other`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.0.iter().all(|feature| other.contains(feature))
    }

    /// Returns the canonical intersection with `other`.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        Self(self.0.iter().filter(|feature| other.contains(feature)).cloned().collect())
    }

    /// Consumes the set and returns its canonical sorted names.
    #[must_use]
    pub fn into_vec(self) -> Vec<ProtocolFeatureName> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_sets_are_sorted_and_duplicate_free() {
        let terminal =
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::TerminalStreaming).unwrap();
        let artifact =
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::ArtifactTransfer).unwrap();
        let set = ProtocolFeatureSet::new(vec![terminal.clone(), artifact], 2).unwrap();
        assert_eq!(set.as_slice()[0].as_str(), ProtocolFeatureName::ARTIFACT_TRANSFER);
        assert!(ProtocolFeatureSet::new(vec![terminal.clone(), terminal], 2).is_err());
    }
}
