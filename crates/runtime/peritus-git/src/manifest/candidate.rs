//! Candidate-tree manifest representation.

use std::path::Path;

use peritus_types::Sha256Digest;

use super::{SCHEMA_VERSION, digest, encoded, finish, format_from_tag, format_tag, path_text};
use crate::{Baseline, CommitId, GitError, Operation, TreeId};

const MAGIC: &str = "peritus-git-candidate";

/// Canonical bytes binding a candidate tree to its repository and observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateTreeManifest {
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl CandidateTreeManifest {
    pub(super) fn new(
        repository: Sha256Digest,
        root: &Path,
        baseline: Baseline,
        head: CommitId,
        tree: TreeId,
        prior: Sha256Digest,
        current: Sha256Digest,
    ) -> Result<Self, GitError> {
        let root = path_text(root, Operation::CreateCandidate)?;
        let mut writer = super::writer();
        let result = (|| {
            writer.write_str(MAGIC)?;
            writer.write_u16(SCHEMA_VERSION)?;
            writer.write_fixed(repository.as_bytes())?;
            writer.write_str(root)?;
            writer.write_u8(format_tag(head.object_id().format()))?;
            super::write_object(&mut writer, baseline.commit().object_id())?;
            super::write_object(&mut writer, baseline.tree().object_id())?;
            super::write_object(&mut writer, head.object_id())?;
            super::write_object(&mut writer, tree.object_id())?;
            writer.write_fixed(prior.as_bytes())?;
            writer.write_fixed(current.as_bytes())
        })();
        let bytes = encoded(result, writer)?;
        Ok(Self { digest: digest(&bytes), bytes })
    }

    /// Decodes one complete schema-v1 candidate manifest.
    ///
    /// # Errors
    ///
    /// Rejects unknown schemas, malformed fields, oversized input, and trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, GitError> {
        let mut reader = super::reader(bytes)?;
        if reader.read_str().map_err(|_| super::invalid_manifest())? != MAGIC
            || reader.read_u16().map_err(|_| super::invalid_manifest())? != SCHEMA_VERSION
        {
            return Err(super::invalid_manifest());
        }
        reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?;
        let root = reader.read_str().map_err(|_| super::invalid_manifest())?;
        if !Path::new(root).is_absolute() {
            return Err(super::invalid_manifest());
        }
        let format = format_from_tag(reader.read_u8().map_err(|_| super::invalid_manifest())?)
            .ok_or_else(super::invalid_manifest)?;
        for _ in 0..4 {
            super::read_object(&mut reader, format)?;
        }
        reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?;
        reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?;
        finish(reader)?;
        Ok(Self { bytes: bytes.to_vec(), digest: digest(bytes) })
    }

    /// Returns exact canonical schema-v1 bytes suitable for durable storage.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the SHA-256 digest of the complete manifest bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
