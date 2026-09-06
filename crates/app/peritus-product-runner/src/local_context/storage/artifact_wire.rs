//! Explicit digest-array wire form without generated custom-field serializer hooks.

use super::StoredArtifact;
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    digest: [u8; 32],
    bytes: u64,
}

impl Serialize for StoredArtifact {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ArtifactWire { digest: self.digest.into_bytes(), bytes: self.bytes }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StoredArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ArtifactWire::deserialize(deserializer)?;
        Ok(Self { digest: Sha256Digest::new(wire.digest), bytes: wire.bytes })
    }
}
