//! Bounded regular-file admission and no-overwrite publication.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use peritus_release_artifacts::ReleasePath;

use super::{OperatorError, binding::decode_hex};

const MAX_EVIDENCE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn read_bounded_regular(path: &Path, label: &str) -> Result<Vec<u8>, OperatorError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| OperatorError::io(&format!("inspect {label}"), path, &source))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_EVIDENCE_BYTES {
        return Err(OperatorError::integrity(format!(
            "{label} must be a regular file no larger than {MAX_EVIDENCE_BYTES} bytes"
        )));
    }
    fs::read(path).map_err(|source| OperatorError::io(&format!("read {label}"), path, &source))
}

pub(super) fn read_exact_material<const N: usize>(
    path: &Path,
    label: &str,
) -> Result<[u8; N], OperatorError> {
    let bytes = read_bounded_regular(path, label)?;
    if bytes.len() == N {
        return bytes.try_into().map_err(|_| OperatorError::integrity("invalid raw length"));
    }
    let text = trim_ascii(&bytes);
    decode_hex::<N>(text)
}

pub(super) fn read_rooted(
    root: &Path,
    path: &ReleasePath,
    label: &str,
) -> Result<Vec<u8>, OperatorError> {
    read_bounded_regular(&root.join(path.as_str()), label)
}

pub(super) fn read_rooted_material<const N: usize>(
    root: &Path,
    path: &ReleasePath,
    label: &str,
) -> Result<[u8; N], OperatorError> {
    read_exact_material(&root.join(path.as_str()), label)
}

pub(super) fn publish_new(path: &Path, bytes: &[u8]) -> Result<(), OperatorError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let metadata = fs::symlink_metadata(parent)
        .map_err(|source| OperatorError::io("inspect output parent", parent, &source))?;
    if !metadata.is_dir() {
        return Err(OperatorError::integrity("output parent must be an existing directory"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| OperatorError::io("create output", path, &source))?;
    file.write_all(bytes).map_err(|source| OperatorError::io("write output", path, &source))?;
    file.sync_all().map_err(|source| OperatorError::io("sync output", path, &source))
}

pub(super) fn publish_bundle(
    output: &Path,
    entries: &[(&str, Vec<u8>)],
) -> Result<(), OperatorError> {
    fs::create_dir(output)
        .map_err(|source| OperatorError::io("create H4 output directory", output, &source))?;
    for (name, bytes) in entries {
        publish_new(&output.join(name), bytes)?;
    }
    Ok(())
}

fn trim_ascii(value: &[u8]) -> &[u8] {
    let start = value.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(value.len());
    let end =
        value.iter().rposition(|byte| !byte.is_ascii_whitespace()).map_or(start, |index| index + 1);
    &value[start..end]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::read_exact_material;

    #[test]
    fn exact_material_accepts_raw_and_lowercase_hex() {
        let root = tempfile::tempdir().expect("tempdir");
        let raw = root.path().join("raw");
        fs::write(&raw, [7_u8; 32]).expect("raw key");
        assert_eq!(read_exact_material::<32>(&raw, "key").expect("raw"), [7; 32]);
        let hex = root.path().join("hex");
        fs::write(&hex, "07".repeat(32)).expect("hex key");
        assert_eq!(read_exact_material::<32>(&hex, "key").expect("hex"), [7; 32]);
    }
}
