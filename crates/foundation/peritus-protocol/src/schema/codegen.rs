//! Filesystem driver for deterministic protocol artifacts.

use super::{generated_agent_binary_artifacts, generated_artifacts, generated_binary_artifacts};
use peritus_codec::sha256;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Generates or checks every protocol artifact from command-line-style arguments.
///
/// # Errors
///
/// Returns an error for unsupported arguments, artifact drift in check mode, or
/// filesystem failures while writing generated outputs.
pub fn run_codegen(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut check = false;
    let mut root = PathBuf::from(".");
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--check") => check = true,
            Some("--root") => {
                root = PathBuf::from(arguments.next().ok_or("--root requires a path")?);
            }
            _ => return Err(format!("unknown argument: {}", argument.to_string_lossy()).into()),
        }
    }

    for artifact in generated_artifacts() {
        write_or_check_text(&root.join(artifact.path), &artifact.content, check)?;
    }
    let binary_artifacts = generated_binary_artifacts()?;
    let mut manifest = String::from("# peritus protocol compatibility corpus v1\n");
    for artifact in &binary_artifacts {
        let digest = sha256(&artifact.content);
        manifest.push_str(&hex(digest.as_bytes()));
        manifest.push_str("  ");
        manifest.push_str(artifact.path);
        manifest.push('\n');
        write_or_check_binary(&root.join(artifact.path), &artifact.content, check)?;
    }
    write_or_check_text(&root.join("protocol/fixtures/v1/SHA256SUMS"), &manifest, check)?;
    let agent_artifacts = generated_agent_binary_artifacts()?;
    let mut agent_manifest = String::from("# peritus agent protocol compatibility corpus v1\n");
    for artifact in &agent_artifacts {
        let digest = sha256(&artifact.content);
        agent_manifest.push_str(&hex(digest.as_bytes()));
        agent_manifest.push_str("  ");
        agent_manifest.push_str(artifact.path);
        agent_manifest.push('\n');
        write_or_check_binary(&root.join(artifact.path), &artifact.content, check)?;
    }
    write_or_check_text(
        &root.join("crates/foundation/peritus-protocol/tests/fixtures/v1/SHA256SUMS"),
        &agent_manifest,
        check,
    )
}

fn write_or_check_text(
    path: &Path,
    expected: &str,
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if check {
        let actual = fs::read_to_string(path).map_err(|error| {
            format!("cannot read generated artifact {}: {error}", path.display())
        })?;
        if actual != expected {
            return Err(format!("generated artifact is stale: {}", path.display()).into());
        }
    } else {
        let parent = path.parent().ok_or("generated artifact has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(path, expected)?;
    }
    Ok(())
}

fn write_or_check_binary(
    path: &Path,
    expected: &[u8],
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if check {
        let actual = fs::read(path).map_err(|error| {
            format!("cannot read compatibility fixture {}: {error}", path.display())
        })?;
        if actual != expected {
            return Err(format!("compatibility fixture is stale: {}", path.display()).into());
        }
    } else {
        let parent = path.parent().ok_or("compatibility fixture has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(path, expected)?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
