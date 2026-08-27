//! Bounded secret-resistant evidence and native execution receipts.

use peritus_types::Sha256Digest;

use crate::{
    QualificationError, QualificationErrorCode, QualificationRecovery, ResourceUsage, digest_bytes,
    error::invalid,
};

/// Maximum structured entries retained for one probe.
pub const MAX_CASE_EVIDENCE_ENTRIES: usize = 96;
/// Maximum encoded structured-evidence bytes retained for one probe.
pub const MAX_CASE_EVIDENCE_BYTES: usize = 256 * 1024;
const MAX_CODE_BYTES: usize = 128;

/// Bounded canonical non-secret code suitable for stable taxonomies and identifiers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SafeEvidenceCode(String);

impl SafeEvidenceCode {
    /// Validates lowercase ASCII segments without whitespace, controls, escapes, or path syntax.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, noncanonical, or secret-shaped values.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_CODE_BYTES
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
            || value.contains("secret")
            || value.contains("token")
            || value.contains("password")
        {
            return Err(invalid("evidence code is not bounded canonical non-secret ASCII"));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Structured evidence value that cannot retain arbitrary process or model text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceValue {
    /// Directly observed Boolean assertion.
    Fact(bool),
    /// Exact bounded count or monotonic measurement.
    Count(u64),
    /// Digest of raw bytes retained in a separately controlled artifact store.
    Digest {
        /// SHA-256 of the external bytes.
        sha256: Sha256Digest,
        /// Exact byte count of the external bytes.
        bytes: u64,
    },
    /// Stable safe taxonomy or identity code.
    Code(SafeEvidenceCode),
}

/// One uniquely labelled structured evidence entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceEntry {
    label: SafeEvidenceCode,
    value: EvidenceValue,
}

impl EvidenceEntry {
    /// Creates a labelled evidence entry.
    #[must_use]
    pub const fn new(label: SafeEvidenceCode, value: EvidenceValue) -> Self {
        Self { label, value }
    }

    /// Borrows the canonical label.
    #[must_use]
    pub const fn label(&self) -> &SafeEvidenceCode {
        &self.label
    }

    /// Borrows the structured evidence value.
    #[must_use]
    pub const fn value(&self) -> &EvidenceValue {
        &self.value
    }

    const fn encoded_size(&self) -> usize {
        self.label.0.len()
            + match &self.value {
                EvidenceValue::Fact(_) => 2,
                EvidenceValue::Count(_) => 9,
                EvidenceValue::Digest { .. } => 41,
                EvidenceValue::Code(value) => value.0.len() + 1,
            }
    }
}

/// Canonically ordered bounded evidence for one native probe.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EvidenceSet {
    entries: Vec<EvidenceEntry>,
    encoded_bytes: usize,
}

impl EvidenceSet {
    /// Creates an empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self { entries: Vec::new(), encoded_bytes: 0 }
    }

    /// Inserts one unique entry while preserving bytewise label order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate labels and count, byte, or arithmetic overflow.
    pub fn insert(&mut self, entry: EvidenceEntry) -> Result<(), QualificationError> {
        if self.entries.len() == MAX_CASE_EVIDENCE_ENTRIES {
            return Err(bound_error("case evidence entry ceiling was reached"));
        }
        let index = self
            .entries
            .binary_search_by(|existing| existing.label.cmp(&entry.label))
            .unwrap_or_else(|index| index);
        if self.entries.get(index).is_some_and(|existing| existing.label == entry.label) {
            return Err(bound_error("case evidence repeats a label"));
        }
        let encoded_bytes = self
            .encoded_bytes
            .checked_add(entry.encoded_size())
            .ok_or_else(|| bound_error("case evidence byte accounting overflowed"))?;
        if encoded_bytes > MAX_CASE_EVIDENCE_BYTES {
            return Err(bound_error("case evidence exceeds its aggregate byte ceiling"));
        }
        self.entries.insert(index, entry);
        self.encoded_bytes = encoded_bytes;
        Ok(())
    }

    /// Borrows entries in canonical label order.
    #[must_use]
    pub fn entries(&self) -> &[EvidenceEntry] {
        &self.entries
    }

    /// Returns aggregate encoded bytes.
    #[must_use]
    pub const fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Reports whether no direct structured evidence was supplied.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Computes a deterministic digest with length-delimited labels and values.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        let mut canonical = Vec::with_capacity(self.encoded_bytes + 32);
        canonical.extend_from_slice(b"peritus/h0/case-evidence/v1\0");
        for entry in &self.entries {
            push_bytes(&mut canonical, entry.label.as_str().as_bytes());
            match &entry.value {
                EvidenceValue::Fact(value) => {
                    canonical.push(0);
                    canonical.push(u8::from(*value));
                }
                EvidenceValue::Count(value) => {
                    canonical.push(1);
                    canonical.extend_from_slice(&value.to_be_bytes());
                }
                EvidenceValue::Digest { sha256, bytes } => {
                    canonical.push(2);
                    canonical.extend_from_slice(sha256.as_bytes());
                    canonical.extend_from_slice(&bytes.to_be_bytes());
                }
                EvidenceValue::Code(value) => {
                    canonical.push(3);
                    push_bytes(&mut canonical, value.as_str().as_bytes());
                }
            }
        }
        digest_bytes(&canonical)
    }
}

/// Direct native-process receipt returned by the host adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeExecutionReceipt {
    executor_digest: Sha256Digest,
    host_fingerprint: Sha256Digest,
    command_digest: Sha256Digest,
    exit_code: i32,
    native_sandbox_observed: bool,
    usage: ResourceUsage,
    evidence: EvidenceSet,
}

impl NativeExecutionReceipt {
    /// Validates nonempty provenance and direct structured evidence.
    ///
    /// This constructor records adapter-supplied native facts; it never runs or simulates a probe.
    ///
    /// # Errors
    ///
    /// Rejects missing provenance digests or empty direct evidence.
    pub fn from_native_observation(
        executor_digest: Sha256Digest,
        host_fingerprint: Sha256Digest,
        command_digest: Sha256Digest,
        exit_code: i32,
        native_sandbox_observed: bool,
        usage: ResourceUsage,
        evidence: EvidenceSet,
    ) -> Result<Self, QualificationError> {
        if !digest_is_present(executor_digest)
            || !digest_is_present(host_fingerprint)
            || !digest_is_present(command_digest)
        {
            return Err(invalid("native execution receipt contains an empty provenance digest"));
        }
        if evidence.is_empty() {
            return Err(invalid(
                "native execution receipt must contain direct structured evidence",
            ));
        }
        Ok(Self {
            executor_digest,
            host_fingerprint,
            command_digest,
            exit_code,
            native_sandbox_observed,
            usage,
            evidence,
        })
    }

    /// Returns the exact native executor binary/configuration digest.
    #[must_use]
    pub const fn executor_digest(&self) -> Sha256Digest {
        self.executor_digest
    }

    /// Returns the native host-image fingerprint.
    #[must_use]
    pub const fn host_fingerprint(&self) -> Sha256Digest {
        self.host_fingerprint
    }

    /// Returns the digest of exact argv, environment policy, and probe fixture.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }

    /// Returns the observed native process exit code.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Reports whether a native sandbox backend was observed when the probe required it.
    #[must_use]
    pub const fn native_sandbox_observed(&self) -> bool {
        self.native_sandbox_observed
    }

    /// Returns direct resource accounting.
    #[must_use]
    pub const fn usage(&self) -> ResourceUsage {
        self.usage
    }

    /// Borrows structured direct evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceSet {
        &self.evidence
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "private observation constructors reuse the evidence digest predicate"
)]
pub(super) fn digest_is_present(digest: Sha256Digest) -> bool {
    digest.as_bytes().iter().any(|byte| *byte != 0)
}

fn push_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn bound_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::EvidenceBound,
        QualificationRecovery::Quarantine,
        "retain H0 case evidence",
        detail,
    )
}
