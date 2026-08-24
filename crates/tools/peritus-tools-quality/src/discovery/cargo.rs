//! Known Cargo project surfaces.

use crate::{CheckDefinition, CheckSource, QualityError, QualityErrorKind};

use super::discovered_definition;

pub(super) fn discover(
    manifest: &[u8],
    definitions: &mut Vec<CheckDefinition>,
) -> Result<(), QualityError> {
    let text = std::str::from_utf8(manifest).map_err(|_| {
        QualityError::new(QualityErrorKind::Parser, "Cargo.toml is not valid UTF-8")
    })?;
    let document = toml::from_str::<toml::Value>(text).map_err(|error| {
        QualityError::new(QualityErrorKind::Parser, format!("Cargo.toml is invalid: {error}"))
    })?;
    if !document.is_table() {
        return Err(QualityError::new(QualityErrorKind::Parser, "Cargo.toml root is not a table"));
    }
    let source = CheckSource::CargoManifest;
    for (name, arguments) in [
        ("cargo.check", vec!["check", "--all-targets", "--all-features"]),
        ("cargo.test", vec!["test", "--all-targets", "--all-features"]),
        ("cargo.clippy", vec!["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"]),
        ("cargo.fmt", vec!["fmt", "--all", "--", "--check"]),
    ] {
        definitions.push(discovered_definition(
            name,
            source.clone(),
            "cargo",
            arguments.into_iter().map(str::to_owned).collect(),
        )?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_manifest_is_not_discovery_success() {
        let error = discover(b"[package\n", &mut Vec::new()).expect_err("invalid TOML");
        assert_eq!(error.kind(), QualityErrorKind::Parser);
    }
}
