//! Atomic no-clobber H1 report publication.

use std::io::Write as _;
use std::path::Path;

pub fn publish(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err("H1 report path already exists".into());
    }
    let parent = path.parent().ok_or("H1 report path has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::publish;
    use std::fs;

    #[test]
    fn publication_is_durable_and_never_overwrites() {
        let root = tempfile::tempdir().expect("temporary root");
        let report = root.path().join("report.json");
        publish(&report, b"first\n").expect("publish report");
        assert_eq!(fs::read(&report).expect("read report"), b"first\n");
        assert!(publish(&report, b"second\n").is_err());
        assert_eq!(fs::read(&report).expect("read retained report"), b"first\n");
    }
}
