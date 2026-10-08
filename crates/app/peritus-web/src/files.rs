//! Bounded directory pages and canonical project-confined file access.

use crate::error::{Result, problem};
use std::path::{Path, PathBuf};

pub mod attachments;
pub mod edit;
pub mod pdf;
mod listing;
pub(crate) mod stream;
pub use listing::list_page;
pub(crate) use listing::{owner_argument as directory_owner_argument, run_owner as run_directory_owner};

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
    let mut bytes = Vec::new();
    let mut file = file;
    let mut window = [0_u8; 8192];
    loop {
        let count = match file.read(&mut window) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        bytes.try_reserve(count).map_err(problem)?;
        bytes.extend_from_slice(&window[..count]);
    }
    validate_text(&bytes)?;
    Ok(bytes)
}

fn validate_text(bytes: &[u8]) -> Result<()> {
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
