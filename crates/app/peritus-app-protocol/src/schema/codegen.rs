//! Filesystem driver for deterministic A3 schemas and compatibility fixtures.

use super::{generated_fixture_cases, generated_text_artifacts};
use peritus_codec::sha256;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Generates or checks every A3 schema and compatibility artifact.
///
/// # Errors
///
/// Returns an error for unsupported arguments, generated-file drift, or filesystem failures.
pub fn run_codegen(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (root, check) = parse_arguments(arguments)?;
    for artifact in generated_text_artifacts() {
        write_or_check(&root.join(artifact.path), artifact.content.as_bytes(), check)?;
    }
    for fixture in generated_fixture_cases()? {
        let directory = root.join("compat/app-protocol/v1").join(fixture.case);
        let expectation = fixture.render_expectation();
        let files = [
            ("expectation.toml", expectation.as_bytes()),
            ("payload.bin", fixture.payload.as_slice()),
        ];
        let manifest = render_manifest(fixture.class.as_str(), fixture.case, &files);
        write_or_check(&directory.join("expectation.toml"), expectation.as_bytes(), check)?;
        write_or_check(&directory.join("payload.bin"), &fixture.payload, check)?;
        write_or_check(&directory.join("fixture.toml"), manifest.as_bytes(), check)?;
    }
    Ok(())
}

fn parse_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<(PathBuf, bool), Box<dyn std::error::Error>> {
    let mut root = PathBuf::from(".");
    let mut check = false;
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
    Ok((root, check))
}

fn render_manifest(class: &str, case: &str, files: &[(&str, &[u8])]) -> String {
    let mut output = format!(
        "schema = 1\nsurface = \"app-protocol\"\nsurface_version = \"v1\"\ncase = \"{case}\"\nkind = \"{class}\"\n"
    );
    for (path, content) in files {
        output.push_str("\n[[files]]\npath = \"");
        output.push_str(path);
        output.push_str("\"\nsha256 = \"");
        output.push_str(&hex(sha256(content).as_bytes()));
        output.push_str("\"\n");
    }
    output
}

fn write_or_check(
    path: &Path,
    expected: &[u8],
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if check {
        let actual = fs::read(path).map_err(|error| {
            format!("cannot read generated artifact {}: {error}", path.display())
        })?;
        if actual != expected {
            return Err(format!("generated artifact is stale: {}", path.display()).into());
        }
    } else {
        fs::create_dir_all(path.parent().ok_or("generated artifact has no parent")?)?;
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

#[cfg(test)]
mod tests {
    use super::render_manifest;

    #[test]
    fn manifest_inventory_is_sorted_and_content_addressed() {
        let manifest = render_manifest(
            "minimal",
            "minimal-client-hello",
            &[("expectation.toml", b"expectation"), ("payload.bin", b"payload")],
        );
        assert!(manifest.contains("surface = \"app-protocol\""));
        assert!(manifest.find("expectation.toml") < manifest.find("payload.bin"));
        assert_eq!(manifest.matches("sha256 = ").count(), 2);
    }
}
