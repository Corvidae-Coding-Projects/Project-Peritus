//! Deterministic SPDX 2.3 SBOM generation from explicit component inputs.

use serde::Serialize;

use crate::{
    ArtifactError, ArtifactErrorCode, BoundedId, ReleaseBinding, Sha256Digest, digest_bytes,
};

const MAX_COMPONENTS: usize = 16_384;

/// Explicit timestamp used by an SPDX creation record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SpdxTimestamp(String);

impl SpdxTimestamp {
    /// Validates a bounded UTC RFC3339-like timestamp supplied by the release environment.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] unless the value is visible ASCII, contains `T`, and ends in `Z`.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        if value.len() < 20
            || value.len() > 40
            || !value.contains('T')
            || !value.ends_with('Z')
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "validate SPDX timestamp",
                "timestamp must be a bounded visible UTC timestamp ending in Z",
            ));
        }
        Ok(Self(value))
    }
}

/// One explicit software component included in the SPDX document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SpdxComponent {
    #[serde(rename = "SPDXID")]
    spdx_id: String,
    name: String,
    #[serde(rename = "versionInfo")]
    version_info: String,
    supplier: String,
    #[serde(rename = "downloadLocation")]
    download_location: String,
    #[serde(rename = "filesAnalyzed")]
    files_analyzed: bool,
    #[serde(rename = "licenseConcluded")]
    license_concluded: String,
    #[serde(rename = "licenseDeclared")]
    license_declared: String,
    checksums: Vec<SpdxChecksum>,
}

impl SpdxComponent {
    /// Creates a package component from explicit release metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when a human-readable field is empty, unsafe, or oversized.
    pub fn new(
        id: &BoundedId,
        name: impl Into<String>,
        version: impl Into<String>,
        supplier: impl Into<String>,
        download_location: impl Into<String>,
        license_expression: impl Into<String>,
        digest: Sha256Digest,
    ) -> Result<Self, ArtifactError> {
        let name = validated_text(name.into(), "component name", 200)?;
        let version_info = validated_text(version.into(), "component version", 120)?;
        let supplier = validated_text(supplier.into(), "component supplier", 240)?;
        let download_location =
            validated_text(download_location.into(), "component download location", 512)?;
        let license = validated_text(license_expression.into(), "license expression", 240)?;
        Ok(Self {
            spdx_id: format!("SPDXRef-Package-{}", id.as_str().replace([':', '/', '@'], "-")),
            name,
            version_info,
            supplier,
            download_location,
            files_analyzed: false,
            license_concluded: license.clone(),
            license_declared: license,
            checksums: vec![SpdxChecksum { algorithm: "SHA256", checksum_value: digest }],
        })
    }

    /// Returns the canonical SPDX element identifier.
    #[must_use]
    pub fn spdx_id(&self) -> &str {
        &self.spdx_id
    }
}

/// Deterministic SPDX 2.3 document for an exact release binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SpdxDocument {
    #[serde(rename = "spdxVersion")]
    spdx_version: &'static str,
    #[serde(rename = "dataLicense")]
    data_license: &'static str,
    #[serde(rename = "SPDXID")]
    spdx_id: &'static str,
    name: String,
    #[serde(rename = "documentNamespace")]
    document_namespace: String,
    #[serde(rename = "creationInfo")]
    creation_info: SpdxCreationInfo,
    packages: Vec<SpdxComponent>,
    relationships: Vec<SpdxRelationship>,
    #[serde(rename = "releaseBinding")]
    release_binding: ReleaseBinding,
}

impl SpdxDocument {
    /// Constructs a complete path-independent SPDX document in canonical component order.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for an empty/oversized list, duplicate SPDX identifiers, or an
    /// invalid creator identity.
    pub fn new(
        binding: ReleaseBinding,
        creator: &BoundedId,
        created: SpdxTimestamp,
        mut components: Vec<SpdxComponent>,
    ) -> Result<Self, ArtifactError> {
        if components.is_empty() || components.len() > MAX_COMPONENTS {
            return Err(ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "create SPDX document",
                "SPDX component list must contain 1 through 16384 entries",
            ));
        }
        components.sort_by(|left, right| left.spdx_id.cmp(&right.spdx_id));
        if let Some(pair) = components.windows(2).find(|pair| pair[0].spdx_id == pair[1].spdx_id) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Duplicate,
                "create SPDX document",
                format!("duplicate SPDX element {}", pair[0].spdx_id),
            ));
        }
        let relationships = components
            .iter()
            .map(|component| SpdxRelationship {
                spdx_element_id: "SPDXRef-DOCUMENT",
                relationship_type: "DESCRIBES",
                related_spdx_element: component.spdx_id.clone(),
            })
            .collect();
        let name = format!("peritus-{}", binding.version().as_str());
        let document_namespace = format!(
            "https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/{}/spdx/{}",
            binding.version().as_str(),
            binding.candidate_commit().as_str()
        );
        Ok(Self {
            spdx_version: "SPDX-2.3",
            data_license: "CC0-1.0",
            spdx_id: "SPDXRef-DOCUMENT",
            name,
            document_namespace,
            creation_info: SpdxCreationInfo {
                created,
                creators: vec![format!("Tool: {}", creator.as_str())],
            },
            packages: components,
            relationships,
            release_binding: binding,
        })
    }

    /// Returns components in canonical SPDX identifier order.
    #[must_use]
    pub fn components(&self) -> &[SpdxComponent] {
        &self.packages
    }

    /// Serializes deterministic compact SPDX JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec(self)
            .map_err(|source| ArtifactError::serialization("serialize SPDX document", source))
    }

    /// Returns the content identity of canonical SPDX JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SpdxChecksum {
    algorithm: &'static str,
    #[serde(rename = "checksumValue")]
    checksum_value: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SpdxCreationInfo {
    created: SpdxTimestamp,
    creators: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SpdxRelationship {
    #[serde(rename = "spdxElementId")]
    spdx_element_id: &'static str,
    #[serde(rename = "relationshipType")]
    relationship_type: &'static str,
    #[serde(rename = "relatedSpdxElement")]
    related_spdx_element: String,
}

fn validated_text(
    value: String,
    field: &'static str,
    maximum: usize,
) -> Result<String, ArtifactError> {
    if value.is_empty()
        || value.len() > maximum
        || value.bytes().any(|byte| byte == 0 || (byte.is_ascii_control() && byte != b'\n'))
    {
        return Err(ArtifactError::new(
            ArtifactErrorCode::InvalidValue,
            "validate SPDX component",
            format!("{field} is empty, unsafe, or exceeds {maximum} bytes"),
        ));
    }
    Ok(value)
}
