//! Strict deserialization models for canonical package-manifest TOML.

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestWire {
    pub(super) schema: u16,
    pub(super) release: String,
    pub(super) platform: String,
    pub(super) architecture: String,
    pub(super) layout_sha256: String,
    pub(super) artifact: Vec<ArtifactWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactWire {
    pub(super) role: String,
    pub(super) path: String,
    pub(super) bytes: u64,
    pub(super) sha256: String,
    pub(super) executable: bool,
}
