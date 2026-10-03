//! Content identity for retrying a commit that only changed Git history.

use super::{ManagedBaseline, failure};
use crate::{ProductRunnerError, file_metadata};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path};

impl ManagedBaseline {
    pub(crate) fn source_digest(root: &Path) -> Result<Sha256Digest, ProductRunnerError> {
        let snapshot = Self::capture(root, false)?;
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-source-v1\0");
        snapshot.hash_source(root, &mut hasher)?;
        Ok(Sha256Digest::new(hasher.finalize().into()))
    }

    fn hash_source(&self, root: &Path, hasher: &mut Sha256) -> Result<(), ProductRunnerError> {
        let metadata = fs::metadata(root).map_err(failure)?;
        hasher.update(file_metadata::permission_fingerprint(&metadata).to_le_bytes());
        for (path, entry) in &self.entries {
            if self.nested.contains_key(path) {
                continue;
            }
            hasher.update([0]);
            let bytes = serde_json::to_vec(&(path, entry)).map_err(failure)?;
            hasher.update(u64::try_from(bytes.len()).map_err(failure)?.to_le_bytes());
            hasher.update(bytes);
        }
        for (path, child) in &self.nested {
            hasher.update([1]);
            hasher.update(u64::try_from(path.len()).map_err(failure)?.to_le_bytes());
            hasher.update(path.as_bytes());
            child.hash_source(&root.join(path), hasher)?;
        }
        hasher.update([2]);
        Ok(())
    }
}
