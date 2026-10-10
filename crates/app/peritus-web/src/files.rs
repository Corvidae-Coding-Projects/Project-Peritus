//! Bounded directory pages and canonical project-confined file access.

use crate::error::{Result, problem};
use std::path::{Path, PathBuf};

pub mod attachments;
pub mod edit;
mod inventory;
pub mod pdf;
pub use inventory::DirectoryCache;
pub mod range;
pub const TEXT_LIMIT: usize = 50 * 1024 * 1024;

pub fn revision(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    crate::state::hex(&Sha256::digest(bytes))
}

pub fn read_text(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(problem("Choose a regular text file"));
    }
    if metadata.len() > TEXT_LIMIT as u64 {
        return Err(problem("This file exceeds the 50 MiB text preview limit. Use Download."));
    }
    let mut bytes = Vec::new();
    file.take((TEXT_LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    validate_text(&bytes)?;
    Ok(bytes)
}

fn validate_text(bytes: &[u8]) -> Result<()> {
    if bytes.len() > TEXT_LIMIT {
        return Err(problem("This file exceeds the 50 MiB text limit. Use Download."));
    }
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return Err(problem(
            "Binary or non-UTF-8 content cannot be viewed or edited as text. Use Download.",
        ));
    }
    Ok(())
}

pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = root.join(relative).canonicalize()?;
    if !candidate.starts_with(root.canonicalize()?) {
        return Err(problem("This path resolves outside the project root"));
    }
    Ok(candidate)
}
pub fn text(root: &Path, relative: &str) -> Result<serde_json::Value> {
    let path = resolve(root, relative)?;
    let bytes = read_text(&path)?;
    let revision = revision(&bytes);
    let length = bytes.len();
    let text = String::from_utf8(bytes).map_err(problem)?;
    Ok(serde_json::json!({"text":text,"revision":revision,"bytes":length}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_does_not_escape_root() {
        let root = tempfile::tempdir().unwrap();
        assert!(resolve(root.path(), "..").is_err());
        std::fs::write(root.path().join("visible.txt"), "hello").unwrap();
        assert!(resolve(root.path(), "visible.txt").is_ok());
    }
    #[test]
    #[cfg(unix)]
    fn symlinks_do_not_escape_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert!(resolve(root.path(), "escape").is_err());
    }
}
