//! Immutable content-addressed harness revisions.

use peritus_types::{HarnessId, RevisionNumber};

use crate::domain::{
    ArtifactRoot, CanonicalEncoder, CanonicalReader, CheckedHarnessGraph, ComponentContents,
    HarnessDomainError, HarnessDomainErrorKind, LineageSeed, ManifestDigest, RevisionDigest,
};

const REVISION_SCHEMA_VERSION: u32 = 1;
const GENESIS_DIGEST_DOMAIN: &[u8] = b"peritus-e1-harness-genesis-digest-v1\0";
const SUCCESSOR_DIGEST_DOMAIN: &[u8] = b"peritus-e1-harness-successor-digest-v1\0";
const REVISION_CANONICAL_DOMAIN: &[u8] = b"peritus-e1-harness-revision-v1\0";

/// Exact E1 harness lineage and full revision identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HarnessRevisionIdentity {
    harness_id: HarnessId,
    number: RevisionNumber,
    digest: RevisionDigest,
}

impl HarnessRevisionIdentity {
    /// Returns the stable lineage identity.
    #[must_use]
    pub const fn harness_id(self) -> HarnessId {
        self.harness_id
    }
    /// Returns the one-based logical revision number.
    #[must_use]
    pub const fn number(self) -> RevisionNumber {
        self.number
    }
    /// Returns the full branch-distinguishing revision digest.
    #[must_use]
    pub const fn digest(self) -> RevisionDigest {
        self.digest
    }
}

/// One complete immutable content-addressed harness revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessRevision {
    harness_id: HarnessId,
    number: RevisionNumber,
    digest: RevisionDigest,
    predecessor: Option<RevisionDigest>,
    lineage_seed: LineageSeed,
    manifest_digest: ManifestDigest,
    graph: CheckedHarnessGraph,
}

impl HarnessRevision {
    /// Creates the first revision after verifying complete exact component bytes.
    ///
    /// # Errors
    ///
    /// Rejects content that does not exactly cover and bind the checked graph.
    pub fn genesis(
        lineage_seed: LineageSeed,
        manifest_digest: ManifestDigest,
        graph: CheckedHarnessGraph,
        contents: &ComponentContents,
    ) -> Result<Self, HarnessDomainError> {
        if !contents.matches_graph(&graph) {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::MissingContent));
        }
        Self::construct_genesis(lineage_seed, manifest_digest, graph)
    }

    /// Creates an immutable direct successor while preserving every protected asset.
    ///
    /// # Errors
    ///
    /// Rejects incomplete contents, revision-number overflow, or any protected-asset drift.
    pub fn successor(
        predecessor: &Self,
        manifest_digest: ManifestDigest,
        graph: CheckedHarnessGraph,
        contents: &ComponentContents,
    ) -> Result<Self, HarnessDomainError> {
        if !contents.matches_graph(&graph) {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::MissingContent));
        }
        Self::construct_successor(predecessor, manifest_digest, graph)
    }

    /// Reconstructs a revision from canonical bytes with an exact predecessor context.
    ///
    /// Passing `None` permits only a valid genesis; passing `Some` permits only that exact direct
    /// successor. The graph is fully decoded and rechecked.
    ///
    /// # Errors
    ///
    /// Rejects malformed bytes, digest disagreement, wrong lineage, or protected drift.
    pub fn decode_canonical(
        bytes: &[u8],
        predecessor: Option<&Self>,
    ) -> Result<Self, HarnessDomainError> {
        let mut reader = CanonicalReader::new(bytes, REVISION_CANONICAL_DOMAIN)?;
        if reader.u32()? != REVISION_SCHEMA_VERSION {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        let encoded_harness_id = reader.harness_id()?;
        let encoded_number = reader.revision_number()?;
        let encoded_digest = RevisionDigest::new(reader.digest()?);
        let encoded_predecessor = reader.optional_digest()?.map(RevisionDigest::new);
        let lineage_seed = LineageSeed::new(reader.digest()?);
        let manifest_digest = ManifestDigest::new(reader.digest()?);
        let graph = CheckedHarnessGraph::decode_canonical(reader.byte_slice()?)?;
        reader.finish()?;
        let revision = match predecessor {
            None => Self::construct_genesis(lineage_seed, manifest_digest, graph)?,
            Some(predecessor) => {
                if lineage_seed != predecessor.lineage_seed {
                    return Err(HarnessDomainError::plain(
                        HarnessDomainErrorKind::HarnessIdentityMismatch,
                    ));
                }
                Self::construct_successor(predecessor, manifest_digest, graph)?
            }
        };
        if revision.harness_id != encoded_harness_id
            || revision.number != encoded_number
            || revision.digest != encoded_digest
            || revision.predecessor != encoded_predecessor
        {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::CanonicalDigestMismatch));
        }
        if revision.canonical_bytes() != bytes {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        Ok(revision)
    }

    pub(crate) fn predecessor_from_canonical(
        bytes: &[u8],
    ) -> Result<Option<RevisionDigest>, HarnessDomainError> {
        let mut reader = CanonicalReader::new(bytes, REVISION_CANONICAL_DOMAIN)?;
        if reader.u32()? != REVISION_SCHEMA_VERSION {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        let _ = reader.harness_id()?;
        let _ = reader.revision_number()?;
        let _ = reader.digest()?;
        reader.optional_digest().map(|value| value.map(RevisionDigest::new))
    }

    fn construct_genesis(
        lineage_seed: LineageSeed,
        manifest_digest: ManifestDigest,
        graph: CheckedHarnessGraph,
    ) -> Result<Self, HarnessDomainError> {
        let number = RevisionNumber::first();
        let digest = revision_digest(
            GENESIS_DIGEST_DOMAIN,
            None,
            number,
            None,
            lineage_seed,
            manifest_digest,
            &graph,
        );
        let harness_id = derive_harness_id(digest)?;
        Ok(Self {
            harness_id,
            number,
            digest,
            predecessor: None,
            lineage_seed,
            manifest_digest,
            graph,
        })
    }

    fn construct_successor(
        predecessor: &Self,
        manifest_digest: ManifestDigest,
        graph: CheckedHarnessGraph,
    ) -> Result<Self, HarnessDomainError> {
        if predecessor.graph.protected_assets() != graph.protected_assets() {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::ProtectedAssetDrift));
        }
        let number = predecessor
            .number
            .checked_next()
            .map_err(|_| HarnessDomainError::plain(HarnessDomainErrorKind::RevisionOverflow))?;
        let digest = revision_digest(
            SUCCESSOR_DIGEST_DOMAIN,
            Some(predecessor.harness_id),
            number,
            Some(predecessor.digest),
            predecessor.lineage_seed,
            manifest_digest,
            &graph,
        );
        Ok(Self {
            harness_id: predecessor.harness_id,
            number,
            digest,
            predecessor: Some(predecessor.digest),
            lineage_seed: predecessor.lineage_seed,
            manifest_digest,
            graph,
        })
    }

    /// Returns the stable harness lineage identity.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns the one-based logical revision number.
    #[must_use]
    pub const fn number(&self) -> RevisionNumber {
        self.number
    }
    /// Returns the authoritative full revision digest.
    #[must_use]
    pub const fn digest(&self) -> RevisionDigest {
        self.digest
    }
    /// Returns the exact direct predecessor digest, absent only for genesis.
    #[must_use]
    pub const fn predecessor(&self) -> Option<RevisionDigest> {
        self.predecessor
    }
    /// Returns the immutable lineage seed.
    #[must_use]
    pub const fn lineage_seed(&self) -> LineageSeed {
        self.lineage_seed
    }
    /// Returns the exact committed manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> ManifestDigest {
        self.manifest_digest
    }
    /// Borrows the complete immutable checked graph.
    #[must_use]
    pub const fn graph(&self) -> &CheckedHarnessGraph {
        &self.graph
    }
    /// Borrows complete component and executable artifact roots.
    #[must_use]
    pub fn artifact_roots(&self) -> &[ArtifactRoot] {
        self.graph.artifact_roots()
    }
    /// Returns the compact exact revision identity used by E1 administrative APIs.
    #[must_use]
    pub const fn identity(&self) -> HarnessRevisionIdentity {
        HarnessRevisionIdentity {
            harness_id: self.harness_id,
            number: self.number,
            digest: self.digest,
        }
    }
    /// Returns whether this revision is the exact direct successor of `predecessor`.
    #[must_use]
    pub fn is_direct_successor_of(&self, predecessor: &Self) -> bool {
        self.harness_id == predecessor.harness_id
            && self.predecessor == Some(predecessor.digest)
            && predecessor.number.checked_next().ok() == Some(self.number)
    }

    /// Returns the deterministic complete schema-v1 revision description.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut encoder = CanonicalEncoder::new(REVISION_CANONICAL_DOMAIN);
        encoder.u32(REVISION_SCHEMA_VERSION);
        encoder.harness_id(self.harness_id);
        encoder.revision_number(self.number);
        encoder.digest(self.digest.digest());
        encoder.optional_digest(self.predecessor.map(RevisionDigest::digest));
        encoder.digest(self.lineage_seed.digest());
        encoder.digest(self.manifest_digest.digest());
        encoder.bytes(&self.graph.canonical_bytes());
        encoder.into_bytes()
    }
}

fn revision_digest(
    domain: &[u8],
    harness_id: Option<HarnessId>,
    number: RevisionNumber,
    predecessor: Option<RevisionDigest>,
    lineage_seed: LineageSeed,
    manifest_digest: ManifestDigest,
    graph: &CheckedHarnessGraph,
) -> RevisionDigest {
    let mut encoder = CanonicalEncoder::new(domain);
    encoder.u32(REVISION_SCHEMA_VERSION);
    encoder.bool(harness_id.is_some());
    if let Some(harness_id) = harness_id {
        encoder.harness_id(harness_id);
    }
    encoder.revision_number(number);
    encoder.optional_digest(predecessor.map(RevisionDigest::digest));
    encoder.digest(lineage_seed.digest());
    encoder.digest(manifest_digest.digest());
    encoder.digest(graph.graph_digest().digest());
    encoder.len(graph.declarations().len());
    for declaration in graph.declarations() {
        declaration.encode(&mut encoder);
    }
    encoder.len(graph.artifact_roots().len());
    for root in graph.artifact_roots() {
        encoder.string(root.component_id().as_str());
        encoder.digest(root.content_digest());
        encoder.optional_digest(
            root.executable_artifact_digest().map(crate::domain::ArtifactDigest::digest),
        );
    }
    RevisionDigest::new(peritus_codec::sha256(&encoder.into_bytes()))
}

fn derive_harness_id(digest: RevisionDigest) -> Result<HarnessId, HarnessDomainError> {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    if bytes.iter().all(|byte| *byte == 0) {
        bytes[15] = 1;
    }
    HarnessId::new(bytes)
        .map_err(|_| HarnessDomainError::plain(HarnessDomainErrorKind::HarnessIdentityMismatch))
}
