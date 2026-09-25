//! Bounded PDF identification before native browser delivery, never HTML sniffing.

use crate::error::{Result, problem};
use std::{io::Read, path::Path};

pub fn inspect(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(problem("Choose a regular PDF file"));
    }
    let mut header = Vec::new();
    std::fs::File::open(path)?.take(1024).read_to_end(&mut header)?;
    // Some readers accept a short preamble. The browser validates the document;
    // this check only confines the PDF-specific delivery policy to PDF input.
    if !header.windows(8).any(|bytes| {
        bytes.starts_with(b"%PDF-")
            && bytes[5].is_ascii_digit()
            && bytes[6] == b'.'
            && bytes[7].is_ascii_digit()
    }) {
        return Err(problem(
            "No PDF header was found. Download this file to inspect it in another application.",
        ));
    }
    Ok(metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identification_is_binary_safe_and_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.PDF");
        let bytes = b"%PDF-1.7\n%\x80\x81\x82\x83\n";
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(inspect(&path).unwrap(), bytes.len() as u64);
        std::fs::write(&path, b"<html>Not a PDF</html>").unwrap();
        assert!(inspect(&path).is_err());
        let mut late_header = vec![b' '; 1024];
        late_header.extend_from_slice(bytes);
        std::fs::write(&path, late_header).unwrap();
        assert!(inspect(&path).is_err());
        assert!(inspect(directory.path()).is_err());
    }
}
