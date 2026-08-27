//! Migration, recovery, rollback, and license-document evidence.

use serde::Serialize;

use crate::{
    ArtifactError, ArtifactErrorCode, BoundedId, MediaType, ReleaseBinding, ReleasePath,
    Sha256Digest, digest_bytes,
};

const MAX_DOCUMENTS: usize = 128;
const MAX_NOTICE_BYTES: usize = 64 * 1024;

/// Required release documentation category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentationKind {
    /// Forward migration procedure and compatibility boundaries.
    Migration,
    /// Pre-migration backup procedure and validation.
    Backup,
    /// Restore procedure and validation.
    Restore,
    /// Failed-upgrade rollback procedure and validation.
    Rollback,
    /// Complete third-party and project license notices.
    LicenseNotices,
    /// Independent security review report retained for the candidate.
    SecurityReview,
}

impl DocumentationKind {
    /// Returns every documentation category required for a complete release inventory.
    #[must_use]
    pub const fn required() -> [Self; 6] {
        [
            Self::Migration,
            Self::Backup,
            Self::Restore,
            Self::Rollback,
            Self::LicenseNotices,
            Self::SecurityReview,
        ]
    }
}

/// Content-addressed documentation observation for one exact release binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentationEvidence {
    binding: ReleaseBinding,
    kind: DocumentationKind,
    path: ReleasePath,
    media_type: MediaType,
    byte_length: u64,
    sha256: Sha256Digest,
}

impl DocumentationEvidence {
    /// Observes a nonempty documentation artifact from exact bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for empty bytes or an unrepresentable byte length.
    pub fn from_bytes(
        binding: ReleaseBinding,
        kind: DocumentationKind,
        path: ReleasePath,
        media_type: MediaType,
        bytes: &[u8],
    ) -> Result<Self, ArtifactError> {
        if bytes.is_empty() {
            return Err(ArtifactError::new(
                ArtifactErrorCode::MissingEvidence,
                "observe release documentation",
                "documentation bytes must not be empty",
            ));
        }
        let byte_length = u64::try_from(bytes.len()).map_err(|_| {
            ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "observe release documentation",
                "documentation length cannot be represented",
            )
        })?;
        Ok(Self { binding, kind, path, media_type, byte_length, sha256: digest_bytes(bytes) })
    }

    /// Returns the documentation category.
    #[must_use]
    pub const fn kind(&self) -> DocumentationKind {
        self.kind
    }

    /// Returns the release-relative documentation path.
    #[must_use]
    pub const fn path(&self) -> &ReleasePath {
        &self.path
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns the document digest.
    #[must_use]
    pub const fn sha256(&self) -> Sha256Digest {
        self.sha256
    }
}

/// Complete canonical inventory of required release documentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentationInventory {
    schema_version: u32,
    binding: ReleaseBinding,
    documents: Vec<DocumentationEvidence>,
}

impl DocumentationInventory {
    /// Validates exactly one document in every required category.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for binding mismatches, duplicates, excessive entries, or a
    /// missing migration, backup, restore, rollback, notice, or security-review document.
    pub fn new(
        binding: ReleaseBinding,
        mut documents: Vec<DocumentationEvidence>,
    ) -> Result<Self, ArtifactError> {
        if documents.len() > MAX_DOCUMENTS {
            return Err(ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "create documentation inventory",
                "documentation inventory exceeds 128 entries",
            ));
        }
        if documents.iter().any(|document| document.binding != binding) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Integrity,
                "create documentation inventory",
                "documentation binds a different release candidate",
            ));
        }
        documents
            .sort_by(|left, right| left.kind.cmp(&right.kind).then(left.path.cmp(&right.path)));
        for required in DocumentationKind::required() {
            let count = documents.iter().filter(|document| document.kind == required).count();
            if count != 1 {
                return Err(ArtifactError::new(
                    if count == 0 {
                        ArtifactErrorCode::MissingEvidence
                    } else {
                        ArtifactErrorCode::Duplicate
                    },
                    "create documentation inventory",
                    format!("documentation category {required:?} must appear exactly once"),
                ));
            }
        }
        Ok(Self { schema_version: 1, binding, documents })
    }

    /// Returns documents in canonical category and path order.
    #[must_use]
    pub fn documents(&self) -> &[DocumentationEvidence] {
        &self.documents
    }

    /// Serializes deterministic compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec(self).map_err(|source| {
            ArtifactError::serialization("serialize documentation inventory", source)
        })
    }

    /// Returns the content identity of canonical JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }
}

/// One component license notice supplied by dependency review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LicenseNotice {
    component: BoundedId,
    version: String,
    license_expression: String,
    notice: String,
}

impl LicenseNotice {
    /// Creates a bounded license notice.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when version, expression, or notice is empty, unsafe, or
    /// oversized.
    pub fn new(
        component: BoundedId,
        version: impl Into<String>,
        license_expression: impl Into<String>,
        notice: impl Into<String>,
    ) -> Result<Self, ArtifactError> {
        let version = notice_text(version.into(), 120, "license version")?;
        let license_expression = notice_text(license_expression.into(), 240, "license expression")?;
        let notice = notice_text(notice.into(), MAX_NOTICE_BYTES, "license notice")?;
        Ok(Self { component, version, license_expression, notice })
    }
}

/// Deterministically rendered complete license-notice document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LicenseNoticeDocument {
    binding: ReleaseBinding,
    notices: Vec<LicenseNotice>,
}

impl LicenseNoticeDocument {
    /// Creates a component-sorted notice document.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for no notices, more than 16384 notices, or duplicate components.
    pub fn new(
        binding: ReleaseBinding,
        mut notices: Vec<LicenseNotice>,
    ) -> Result<Self, ArtifactError> {
        if notices.is_empty() || notices.len() > 16_384 {
            return Err(ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "create license notice document",
                "notice list must contain 1 through 16384 entries",
            ));
        }
        notices.sort_by(|left, right| left.component.cmp(&right.component));
        if let Some(pair) = notices.windows(2).find(|pair| pair[0].component == pair[1].component) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Duplicate,
                "create license notice document",
                format!("duplicate license component {}", pair[0].component),
            ));
        }
        Ok(Self { binding, notices })
    }

    /// Renders deterministic Markdown suitable for release retention.
    #[must_use]
    pub fn render_markdown(&self) -> String {
        let mut output = format!(
            "# Peritus {} license notices\n\nCandidate commit: `{}`\n\n",
            self.binding.version().as_str(),
            self.binding.candidate_commit().as_str()
        );
        for notice in &self.notices {
            output.push_str("## ");
            output.push_str(notice.component.as_str());
            output.push(' ');
            output.push_str(&notice.version);
            output.push_str("\n\nLicense: `");
            output.push_str(&notice.license_expression);
            output.push_str("`\n\n");
            output.push_str(&notice.notice);
            if !notice.notice.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
        }
        output
    }
}

fn notice_text(
    value: String,
    maximum: usize,
    field: &'static str,
) -> Result<String, ArtifactError> {
    if value.is_empty()
        || value.len() > maximum
        || value
            .bytes()
            .any(|byte| byte == 0 || (byte.is_ascii_control() && !matches!(byte, b'\n' | b'\t')))
    {
        return Err(ArtifactError::new(
            ArtifactErrorCode::InvalidValue,
            "validate license notice",
            format!("{field} is empty, unsafe, or exceeds {maximum} bytes"),
        ));
    }
    Ok(value)
}
