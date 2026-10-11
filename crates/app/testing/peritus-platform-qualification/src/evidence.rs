//! Bounded, secret-free H2 evidence values.

use crate::{QualificationError, QualificationErrorCode, QualificationRecovery, Sha256Digest};

/// Maximum entries retained for one scenario.
pub const MAX_EVIDENCE_ENTRIES: usize = 64;
/// Maximum aggregate label/value bytes retained for one scenario.
pub const MAX_EVIDENCE_BYTES: usize = 256 * 1024;
const MAX_LABEL_BYTES: usize = 96;
const MAX_TEXT_BYTES: usize = 8 * 1024;

/// Validated bounded evidence text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceText(String);

impl EvidenceText {
    /// Creates bounded UTF-8 evidence after rejecting NUL and escape/control bytes.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or unsafe diagnostic text.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_TEXT_BYTES
            || value.bytes().any(|byte| byte == 0 || (byte.is_ascii_control() && byte != b'\n'))
        {
            return Err(evidence_error("evidence text exceeds its safe display bound"));
        }
        Ok(Self(value))
    }

    /// Borrows the validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed evidence representation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceKind {
    /// Bounded non-secret diagnostic or declarative value.
    Text(EvidenceText),
    /// Digest of raw output retained outside the report.
    Digest {
        /// Exact SHA-256 of the externally retained bytes.
        sha256: Sha256Digest,
        /// Exact externally retained byte count.
        bytes: u64,
    },
    /// Monotonic bounded quantity.
    Count(u64),
    /// Boolean fact observed directly by the adapter.
    Fact(bool),
}

/// One stable labelled evidence fact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceEntry {
    label: String,
    kind: EvidenceKind,
}

impl EvidenceEntry {
    /// Creates a labelled evidence fact.
    ///
    /// # Errors
    ///
    /// Rejects labels outside the lowercase dotted identifier grammar.
    pub fn new(label: impl Into<String>, kind: EvidenceKind) -> Result<Self, QualificationError> {
        let label = label.into();
        if label.is_empty()
            || label.len() > MAX_LABEL_BYTES
            || !label.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
        {
            return Err(evidence_error("evidence label is not canonical"));
        }
        Ok(Self { label, kind })
    }

    /// Borrows the canonical label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Borrows the evidence value.
    #[must_use]
    pub const fn kind(&self) -> &EvidenceKind {
        &self.kind
    }

    const fn byte_len(&self) -> usize {
        self.label.len()
            + match &self.kind {
                EvidenceKind::Text(text) => text.0.len(),
                EvidenceKind::Digest { .. } => 40,
                EvidenceKind::Count(_) => 8,
                EvidenceKind::Fact(_) => 1,
            }
    }
}

/// Canonically ordered evidence for one scenario.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EvidenceSet {
    entries: Vec<EvidenceEntry>,
    bytes: usize,
}

impl EvidenceSet {
    /// Creates an empty evidence set.
    #[must_use]
    pub const fn new() -> Self {
        Self { entries: Vec::new(), bytes: 0 }
    }

    /// Inserts one unique labelled entry while maintaining canonical order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate labels or aggregate count/byte overflow.
    pub fn insert(&mut self, entry: EvidenceEntry) -> Result<(), QualificationError> {
        if self.entries.len() == MAX_EVIDENCE_ENTRIES {
            return Err(evidence_error("scenario evidence entry bound is exhausted"));
        }
        let index = self
            .entries
            .binary_search_by(|existing| existing.label.cmp(&entry.label))
            .unwrap_or_else(|index| index);
        if self.entries.get(index).is_some_and(|existing| existing.label == entry.label) {
            return Err(evidence_error("scenario evidence repeats a label"));
        }
        let bytes = self
            .bytes
            .checked_add(entry.byte_len())
            .ok_or_else(|| evidence_error("scenario evidence byte accounting overflowed"))?;
        if bytes > MAX_EVIDENCE_BYTES {
            return Err(evidence_error("scenario evidence exceeds its aggregate byte bound"));
        }
        self.entries.insert(index, entry);
        self.bytes = bytes;
        Ok(())
    }

    /// Borrows entries in canonical label order.
    #[must_use]
    pub fn entries(&self) -> &[EvidenceEntry] {
        &self.entries
    }

    /// Returns aggregate retained bytes.
    #[must_use]
    pub const fn byte_len(&self) -> usize {
        self.bytes
    }

    /// Returns whether no evidence has been recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns a deterministic digest over labels, variants, and values.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        let mut canonical = String::from("peritus/h2-scenario-evidence/v1\n");
        for entry in &self.entries {
            use core::fmt::Write as _;
            let _ = match &entry.kind {
                EvidenceKind::Text(value) => {
                    writeln!(&mut canonical, "{}|text|{}", entry.label, value.as_str())
                }
                EvidenceKind::Digest { sha256, bytes } => {
                    writeln!(&mut canonical, "{}|digest|{}|{}", entry.label, sha256, bytes)
                }
                EvidenceKind::Count(value) => {
                    writeln!(&mut canonical, "{}|count|{}", entry.label, value)
                }
                EvidenceKind::Fact(value) => {
                    writeln!(&mut canonical, "{}|fact|{}", entry.label, value)
                }
            };
        }
        crate::digest_bytes(canonical.as_bytes()).sha256()
    }
}

fn evidence_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::EvidenceBound,
        QualificationRecovery::Quarantine,
        "validate qualification evidence",
        detail,
    )
}
