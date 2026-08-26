//! Stable pattern fingerprints derived only from typed facts.

use crate::{AnalysisFinding, DebuggerError, TraceSelectionManifest};
use peritus_harness::domain::ComponentKind;
use peritus_types::Sha256Digest;

/// Digest of a normalized typed pattern signature, never free-form prose.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternFingerprint(Sha256Digest);

impl PatternFingerprint {
    pub(super) fn for_finding(
        finding: &AnalysisFinding,
        manifest: &TraceSelectionManifest,
        component_kind: Option<ComponentKind>,
    ) -> Result<Self, DebuggerError> {
        let subject = manifest
            .subjects()
            .iter()
            .find(|subject| subject.id() == finding.subject_id())
            .ok_or_else(|| {
                DebuggerError::new(
                    crate::DebuggerErrorKind::Binding,
                    crate::DebuggerOperation::ClusterPatterns,
                    crate::DebuggerRecovery::RepairDependency,
                    "analysis finding subject is absent from the selection manifest",
                )
            })?;
        let mut bytes = b"peritus-e2-pattern-fingerprint-v1\0".to_vec();
        bytes.extend_from_slice(&finding.outcome().tag().to_be_bytes());
        bytes.extend_from_slice(
            &finding.category().map_or(0, crate::FailureCategory::tag).to_be_bytes(),
        );
        bytes.push(finding.signature() as u8);
        bytes.extend_from_slice(subject.environment_id().as_bytes());
        bytes.extend_from_slice(subject.harness_revision().digest().as_bytes());
        bytes.extend_from_slice(&subject.revision().workspace_revision().get().to_be_bytes());
        bytes.push(component_kind.map_or(0, ComponentKind::tag));
        bytes.extend_from_slice(&normalized_causal_shape(finding, manifest));
        Ok(Self(crate::identity::domain_digest(
            b"peritus-e2-pattern-fingerprint-digest-v1\0",
            &bytes,
        )))
    }

    pub(super) fn combined(fingerprints: &[Self]) -> Self {
        let mut bytes = b"peritus-e2-combined-pattern-fingerprint-v1\0".to_vec();
        crate::query::encode_len(&mut bytes, fingerprints.len());
        for fingerprint in fingerprints {
            bytes.extend_from_slice(fingerprint.0.as_bytes());
        }
        Self(crate::identity::domain_digest(
            b"peritus-e2-combined-pattern-fingerprint-digest-v1\0",
            &bytes,
        ))
    }

    /// Returns the exact SHA-256 fingerprint.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}

fn normalized_causal_shape(
    finding: &AnalysisFinding,
    manifest: &TraceSelectionManifest,
) -> [u8; 24] {
    let mut edges = 0_u64;
    let mut missing = 0_u64;
    let mut spans = std::collections::BTreeSet::new();
    for citation in finding.citations() {
        if let Some(entry) = manifest.event(citation.event_id()) {
            edges = edges
                .saturating_add(u64::try_from(entry.causal_events().len()).unwrap_or(u64::MAX));
            spans.insert(entry.span_id());
            missing = missing.saturating_add(u64::from(
                entry.causal_events().iter().any(|event| manifest.event(*event).is_none()),
            ));
        }
    }
    let mut bytes = [0_u8; 24];
    bytes[..8].copy_from_slice(&edges.to_be_bytes());
    bytes[8..16].copy_from_slice(&missing.to_be_bytes());
    bytes[16..].copy_from_slice(&u64::try_from(spans.len()).unwrap_or(u64::MAX).to_be_bytes());
    bytes
}
