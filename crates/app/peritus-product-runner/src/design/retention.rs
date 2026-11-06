//! Private recovery copies are evidence only; publication still validates current inputs.

use std::{fs, io::Write as _, path::Path};

use serde_json::Value;

use crate::{ProductRunInput, ProductRunnerError, ProductRunnerErrorKind};

const MAX_BYTES: u64 = 6 * 1024 * 1024;

pub(super) fn archive(input: &ProductRunInput) -> Result<(), ProductRunnerError> {
    for extension in ["design-source.json", "design-expansion.json"] {
        let path = input.trace_path.with_extension(extension);
        let Some(bytes) = read(&path)? else { continue };
        let digest = crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes());
        write(&input.trace_path.with_extension(format!("{extension}.{digest}.history")), &bytes)?;
        if extension == "design-expansion.json" {
            write(&input.trace_path.with_extension("design-prior-state.json"), &bytes)?;
        }
    }
    Ok(())
}

pub(super) fn proposal(
    input: &ProductRunInput,
    arguments: &Value,
) -> Result<(), ProductRunnerError> {
    let value = serde_json::json!({
        "schema":1,
        "authority":"unvalidated_proposal_only",
        "input_revision":input.conversation.incorporated_revision(),
        "arguments":arguments,
    });
    let bytes = serde_json::to_vec(&value).map_err(|e| failure(e.to_string()))?;
    let digest = crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes());
    write(&input.trace_path.with_extension(format!("design-proposal-{digest}.json")), &bytes)?;
    write(&input.trace_path.with_extension("design-proposal.json"), &bytes)
}

pub(super) fn observation(input: &ProductRunInput) -> Result<Option<Value>, ProductRunnerError> {
    read(&input.trace_path.with_extension("design-proposal.json"))?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string())))
        .transpose()
}

pub(super) fn previous_state(input: &ProductRunInput) -> Result<Option<Value>, ProductRunnerError> {
    read(&input.trace_path.with_extension("design-prior-state.json"))?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string())))
        .transpose()
}

pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>, ProductRunnerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failure(error.to_string())),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_BYTES {
        return Err(failure("invalid retained design recovery artifact"));
    }
    super::expansion::read_stable_regular(path, MAX_BYTES, "design recovery artifact").map(Some)
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), ProductRunnerError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(failure("design recovery artifact exceeds its size bound"));
    }
    let parent = path.parent().ok_or_else(|| failure("recovery artifact has no parent"))?;
    fs::create_dir_all(parent).map_err(|e| failure(e.to_string()))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| failure(e.to_string()))?;
    temporary.write_all(bytes).map_err(|e| failure(e.to_string()))?;
    temporary.as_file().sync_all().map_err(|e| failure(e.to_string()))?;
    temporary.persist(path).map_err(|e| failure(e.to_string()))?;
    #[cfg(unix)]
    fs::File::open(parent).and_then(|file| file.sync_all()).map_err(|e| failure(e.to_string()))?;
    Ok(())
}

fn failure(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "retain design recovery evidence",
        detail,
    )
}
