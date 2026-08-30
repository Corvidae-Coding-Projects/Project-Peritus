//! Validated release-binding interchange.

use std::path::Path;

use serde::Deserialize;

use peritus_release_artifacts::{
    CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion, Sha256Digest, ToolchainId,
};

use super::plan::BindingSpec;
use super::{OperatorError, files};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingDocument {
    candidate_commit: String,
    version: String,
    toolchain: String,
    platform: String,
    source_tree_digest: String,
}

pub(super) fn read(path: &Path) -> Result<ReleaseBinding, OperatorError> {
    let bytes = files::read_bounded_regular(path, "release binding")?;
    let document: BindingDocument = serde_json::from_slice(&bytes)?;
    from_parts(
        document.candidate_commit,
        document.version,
        document.toolchain,
        document.platform,
        &document.source_tree_digest,
    )
}

pub(super) fn from_spec(spec: &BindingSpec) -> Result<ReleaseBinding, OperatorError> {
    from_parts(
        spec.candidate_commit.clone(),
        spec.version.clone(),
        spec.toolchain.clone(),
        spec.platform.clone(),
        &spec.source_tree_digest,
    )
}

fn from_parts(
    commit: String,
    version: String,
    toolchain: String,
    platform: String,
    source_digest: &str,
) -> Result<ReleaseBinding, OperatorError> {
    Ok(ReleaseBinding::new(
        CandidateCommit::new(commit)?,
        ReleaseVersion::new(version)?,
        ToolchainId::new(toolchain)?,
        PlatformTriple::new(platform)?,
        parse_digest(source_digest)?,
    ))
}

fn parse_digest(value: &str) -> Result<Sha256Digest, OperatorError> {
    let bytes = decode_hex::<32>(value.as_bytes())?;
    Ok(Sha256Digest::from_bytes(bytes))
}

pub(super) fn decode_hex<const N: usize>(value: &[u8]) -> Result<[u8; N], OperatorError> {
    if value.len() != N * 2 {
        return Err(OperatorError::integrity(format!(
            "expected {} lowercase hexadecimal characters",
            N * 2
        )));
    }
    let mut decoded = [0_u8; N];
    for (index, pair) in value.chunks_exact(2).enumerate() {
        decoded[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(decoded)
}

fn nibble(value: u8) -> Result<u8, OperatorError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(OperatorError::integrity(
            "public material must use lowercase hexadecimal when not raw",
        )),
    }
}
