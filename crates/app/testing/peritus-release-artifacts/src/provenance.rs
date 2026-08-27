//! Deterministic in-toto/SLSA-style provenance statements.

use serde::Serialize;

use crate::{
    ArtifactError, ArtifactErrorCode, ArtifactInventory, BoundedId, ReleaseBinding, Sha256Digest,
    SpdxTimestamp, digest_bytes,
};

/// SLSA v1 provenance predicate type emitted by this crate.
pub const SLSA_PROVENANCE_PREDICATE_TYPE: &str = "https://slsa.dev/provenance/v1";

/// One immutable input material used by a release builder.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildMaterial {
    uri: String,
    digest: ProvenanceDigest,
}

impl BuildMaterial {
    /// Creates a material with an explicit URI and SHA-256 digest.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when the URI is empty, unsafe, or exceeds 512 bytes.
    pub fn new(uri: impl Into<String>, sha256: Sha256Digest) -> Result<Self, ArtifactError> {
        let uri = uri.into();
        if uri.is_empty() || uri.len() > 512 || !uri.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "validate provenance material",
                "material URI must contain 1 through 512 visible ASCII bytes",
            ));
        }
        Ok(Self { uri, digest: ProvenanceDigest { sha256 } })
    }

    /// Borrows the material URI.
    #[must_use]
    pub fn uri(&self) -> &str {
        &self.uri
    }
}

/// Explicit build start and finish timestamps.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProvenanceTimestamps {
    #[serde(rename = "startedOn")]
    started_on: SpdxTimestamp,
    #[serde(rename = "finishedOn")]
    finished_on: SpdxTimestamp,
}

impl ProvenanceTimestamps {
    /// Creates caller-observed build timestamps.
    #[must_use]
    pub const fn new(started_on: SpdxTimestamp, finished_on: SpdxTimestamp) -> Self {
        Self { started_on, finished_on }
    }
}

/// SLSA-style statement bound to every inventoried release subject.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProvenanceStatement {
    #[serde(rename = "_type")]
    statement_type: &'static str,
    subject: Vec<ProvenanceSubject>,
    #[serde(rename = "predicateType")]
    predicate_type: &'static str,
    predicate: ProvenancePredicate,
}

impl ProvenanceStatement {
    /// Creates a deterministic provenance statement from explicit build observations.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for no materials, duplicate material URIs, or a binding mismatch.
    pub fn new(
        binding: ReleaseBinding,
        inventory: &ArtifactInventory,
        builder_id: BoundedId,
        invocation_id: BoundedId,
        build_type: BoundedId,
        timestamps: ProvenanceTimestamps,
        mut materials: Vec<BuildMaterial>,
    ) -> Result<Self, ArtifactError> {
        if inventory.binding() != &binding {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Integrity,
                "create provenance statement",
                "artifact inventory does not bind the provenance candidate",
            ));
        }
        if materials.is_empty() || materials.len() > 16_384 {
            return Err(ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "create provenance statement",
                "provenance must contain 1 through 16384 materials",
            ));
        }
        materials.sort_by(|left, right| left.uri.cmp(&right.uri));
        if let Some(pair) = materials.windows(2).find(|pair| pair[0].uri == pair[1].uri) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Duplicate,
                "create provenance statement",
                format!("duplicate provenance material URI {}", pair[0].uri),
            ));
        }
        let subject = inventory
            .artifacts()
            .iter()
            .map(|artifact| ProvenanceSubject {
                name: artifact.path().as_str().to_owned(),
                digest: ProvenanceDigest { sha256: artifact.sha256() },
            })
            .collect();
        Ok(Self {
            statement_type: "https://in-toto.io/Statement/v1",
            subject,
            predicate_type: SLSA_PROVENANCE_PREDICATE_TYPE,
            predicate: ProvenancePredicate {
                build_definition: BuildDefinition {
                    build_type,
                    external_parameters: binding,
                    resolved_dependencies: materials,
                },
                run_details: RunDetails {
                    builder: Builder { id: builder_id },
                    metadata: BuildMetadata { invocation_id, timestamps },
                },
            },
        })
    }

    /// Serializes deterministic compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec(self).map_err(|source| {
            ArtifactError::serialization("serialize provenance statement", source)
        })
    }

    /// Returns the SHA-256 identity of canonical JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ProvenanceSubject {
    name: String,
    digest: ProvenanceDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct ProvenanceDigest {
    sha256: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ProvenancePredicate {
    #[serde(rename = "buildDefinition")]
    build_definition: BuildDefinition,
    #[serde(rename = "runDetails")]
    run_details: RunDetails,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct BuildDefinition {
    #[serde(rename = "buildType")]
    build_type: BoundedId,
    #[serde(rename = "externalParameters")]
    external_parameters: ReleaseBinding,
    #[serde(rename = "resolvedDependencies")]
    resolved_dependencies: Vec<BuildMaterial>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RunDetails {
    builder: Builder,
    metadata: BuildMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Builder {
    id: BoundedId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct BuildMetadata {
    #[serde(rename = "invocationId")]
    invocation_id: BoundedId,
    #[serde(flatten)]
    timestamps: ProvenanceTimestamps,
}
