//! UTF-8 byte-range previews without whole-file reads or hashing.

use crate::error::{Result, problem};
use serde_json::{Value, json};
use std::{
    fs::Metadata,
    io::{Read as _, Seek as _, SeekFrom},
    path::Path,
};

const PAGE_BYTES: usize = 64 * 1024;

pub fn text(root: &Path, relative: &str, offset: u64, expected: &str) -> Result<Value> {
    let path = super::resolve(root, relative)?;
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(problem("Choose a regular text file"));
    }
    let version = version(&metadata)?;
    if !expected.is_empty() && expected != version {
        return Err(problem("File changed while paging; reload its first page"));
    }
    if offset > metadata.len() {
        return Err(problem("Text offset is past the end of the file"));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.by_ref().take(u64::try_from(PAGE_BYTES + 3).map_err(problem)?).read_to_end(&mut bytes)?;
    let mut start = 0;
    if offset != 0 {
        while start < bytes.len() && bytes[start] & 0xc0 == 0x80 && start < 3 {
            start += 1;
        }
    }
    let remaining = &bytes[start..];
    let mut count = remaining.len().min(PAGE_BYTES);
    while count < remaining.len() && remaining[count] & 0xc0 == 0x80 {
        count += 1;
    }
    let text = std::str::from_utf8(&remaining[..count])
        .map_err(|_| problem("This range is not UTF-8 text; use Download"))?;
    if text.contains('\0') {
        return Err(problem("This range contains binary data; use Download"));
    }
    if version != self::version(&file.metadata()?)? {
        return Err(problem("File changed during the read; reload its first page"));
    }
    let start = offset
        .checked_add(u64::try_from(start).map_err(problem)?)
        .ok_or_else(|| problem("Text offset overflow"))?;
    let end = start
        .checked_add(u64::try_from(count).map_err(problem)?)
        .ok_or_else(|| problem("Text offset overflow"))?;
    Ok(
        json!({"text":text, "bytes":metadata.len(), "offset":start, "next":(end < metadata.len()).then_some(end),
        "version":version, "revision": if start == 0 && end == metadata.len() { super::revision(text.as_bytes()) } else { String::new() }}),
    )
}

fn version(metadata: &Metadata) -> Result<String> {
    let identity =
        format!("{}:{:?}:{:?}", metadata.len(), metadata.modified()?, metadata.created().ok());
    #[cfg(unix)]
    let identity = {
        use std::fmt::Write as _;
        use std::os::unix::fs::MetadataExt as _;
        let mut identity = identity;
        write!(
            identity,
            ":{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        )
        .map_err(problem)?;
        identity
    };
    Ok(super::revision(identity.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn range_reads_cross_utf8_boundary_and_large_file_tail_without_materializing_prefix() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("large.txt");
        let text = format!("{}é😀tail", "x".repeat(PAGE_BYTES - 1));
        std::fs::write(&path, &text).expect("fixture");
        let first = self::text(root.path(), "large.txt", 0, "").expect("first");
        let next = first["next"].as_u64().expect("next");
        let last =
            self::text(root.path(), "large.txt", next, first["version"].as_str().expect("version"))
                .expect("last");
        assert_eq!(
            format!(
                "{}{}",
                first["text"].as_str().expect("text"),
                last["text"].as_str().expect("text")
            ),
            text
        );
        assert!(last["next"].is_null());
        let huge = std::fs::OpenOptions::new().write(true).open(&path).expect("file");
        huge.set_len(1024 * 1024 * 1024).expect("sparse prefix");
        let mut huge = huge;
        huge.seek(SeekFrom::End(0)).expect("end");
        huge.write_all(b"tail").expect("tail");
        let tail =
            self::text(root.path(), "large.txt", 1024 * 1024 * 1024, "").expect("tail range");
        assert_eq!(tail["text"], "tail");
        assert!(
            self::text(root.path(), "large.txt", next, first["version"].as_str().expect("version"))
                .is_err()
        );
    }
}
