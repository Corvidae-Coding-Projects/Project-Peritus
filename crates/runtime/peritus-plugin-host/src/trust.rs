//! Explicit exact-digest plugin trust policy.

use std::collections::BTreeMap;

use peritus_plugin_sdk::{ManifestDigest, PluginId, PluginVersion};

use crate::DiscoveredPlugin;

/// Result of checking manifest and artifact trust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrustDecision {
    /// Exact canonical manifest and artifact digests match an explicit trust anchor.
    Trusted {
        /// User-visible trust anchor name.
        anchor: String,
    },
    /// No trust anchor exists for this plugin version.
    Unknown,
    /// A trust anchor exists but exact bytes changed.
    DigestMismatch,
}

/// Read-only trust verifier invoked before every plugin start.
pub trait TrustVerifier: Send + Sync {
    /// Verifies exact discovered bytes against configured trust state.
    fn verify(&self, plugin: &DiscoveredPlugin) -> TrustDecision;
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TrustKey {
    id: PluginId,
    version: PluginVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TrustAnchor {
    name: String,
    manifest: ManifestDigest,
    artifact: [u8; 32],
}

/// Immutable allowlist of exact manifest/artifact digest pairs.
#[derive(Clone, Debug, Default)]
pub struct DigestTrustStore {
    anchors: BTreeMap<TrustKey, TrustAnchor>,
}

impl DigestTrustStore {
    /// Creates an empty deny-by-default trust store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one exact trust anchor.
    #[must_use]
    pub fn with_anchor(
        mut self,
        id: PluginId,
        version: PluginVersion,
        manifest: ManifestDigest,
        artifact: [u8; 32],
        name: impl Into<String>,
    ) -> Self {
        self.anchors.insert(
            TrustKey { id, version },
            TrustAnchor { name: name.into(), manifest, artifact },
        );
        self
    }
}

impl TrustVerifier for DigestTrustStore {
    fn verify(&self, plugin: &DiscoveredPlugin) -> TrustDecision {
        let key =
            TrustKey { id: plugin.manifest().id().clone(), version: plugin.manifest().version() };
        let Some(anchor) = self.anchors.get(&key) else {
            return TrustDecision::Unknown;
        };
        if anchor.manifest == plugin.manifest_digest()
            && anchor.artifact == plugin.artifact_sha256()
        {
            TrustDecision::Trusted { anchor: anchor.name.clone() }
        } else {
            TrustDecision::DigestMismatch
        }
    }
}
