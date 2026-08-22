//! Strict fixture manifest parsing and digest representation.

use super::{FixtureError, FixtureErrorKind, FixtureName, FixturePath, FixtureVersion};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use std::path::Path;

/// The mandated semantic role of a compatibility case.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum FixtureKind {
    /// The smallest valid representation of one schema version.
    Minimal,
    /// A representative production-scale shape for one schema version.
    Realistic,
    /// Structurally or semantically corrupt input expected to be rejected.
    Corrupt,
    /// Hostile boundary input exercising defensive behavior.
    Adversarial,
}

/// One exact file declared by a fixture manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureFile {
    path: FixturePath,
    sha256: Sha256Digest,
}

impl FixtureFile {
    /// Returns the portable path relative to the case directory.
    #[must_use]
    pub const fn path(&self) -> &FixturePath {
        &self.path
    }

    /// Returns the declared exact digest bytes.
    #[must_use]
    pub const fn sha256(&self) -> Sha256Digest {
        self.sha256
    }
}

/// A validated version-one compatibility fixture manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureManifest {
    surface: FixtureName,
    surface_version: FixtureVersion,
    case: FixtureName,
    kind: FixtureKind,
    files: Vec<FixtureFile>,
}

impl FixtureManifest {
    pub(crate) fn parse(contents: &str, path: &Path) -> Result<Self, FixtureError> {
        let raw: RawManifest = toml::from_str(contents).map_err(|source| {
            FixtureError::sourced(
                FixtureErrorKind::ManifestSyntax,
                path,
                "fixture manifest did not match its strict schema",
                source,
            )
        })?;
        if raw.schema != 1 {
            return Err(FixtureError::at(
                FixtureErrorKind::UnsupportedSchema,
                path,
                format!("unsupported fixture schema {}", raw.schema),
            ));
        }
        let mut files = Vec::with_capacity(raw.files.len());
        for raw_file in raw.files {
            if raw_file.path == "fixture.toml" {
                return Err(FixtureError::at(
                    FixtureErrorKind::NonCanonicalFiles,
                    path,
                    "fixture.toml cannot list itself as payload",
                ));
            }
            files.push(FixtureFile {
                path: FixturePath::new(raw_file.path)?,
                sha256: parse_digest(&raw_file.sha256, path)?,
            });
        }
        if files.is_empty() {
            return Err(FixtureError::at(
                FixtureErrorKind::NonCanonicalFiles,
                path,
                "a fixture manifest must declare at least one payload file",
            ));
        }
        if files.windows(2).any(|pair| pair[0].path >= pair[1].path) {
            return Err(FixtureError::at(
                FixtureErrorKind::NonCanonicalFiles,
                path,
                "manifest file entries must be strictly sorted and unique",
            ));
        }
        Ok(Self {
            surface: FixtureName::new(raw.surface)?,
            surface_version: FixtureVersion::new(raw.surface_version)?,
            case: FixtureName::new(raw.case)?,
            kind: raw.kind,
            files,
        })
    }

    /// Returns the compatibility surface name.
    #[must_use]
    pub const fn surface(&self) -> &FixtureName {
        &self.surface
    }

    /// Returns the opaque surface version.
    #[must_use]
    pub const fn surface_version(&self) -> &FixtureVersion {
        &self.surface_version
    }

    /// Returns the case name.
    #[must_use]
    pub const fn case(&self) -> &FixtureName {
        &self.case
    }

    /// Returns the mandated case role.
    #[must_use]
    pub const fn kind(&self) -> FixtureKind {
        self.kind
    }

    /// Returns declared files in canonical path order.
    #[must_use]
    pub fn files(&self) -> &[FixtureFile] {
        &self.files
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema: u32,
    surface: String,
    surface_version: String,
    case: String,
    kind: FixtureKind,
    files: Vec<RawFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    path: String,
    sha256: String,
}

fn parse_digest(value: &str, path: &Path) -> Result<Sha256Digest, FixtureError> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(FixtureError::at(
            FixtureErrorKind::InvalidDigest,
            path,
            "SHA-256 must contain exactly 64 lowercase hexadecimal bytes",
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
    }
    Ok(Sha256Digest::new(bytes))
}

const fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0,
    }
}
