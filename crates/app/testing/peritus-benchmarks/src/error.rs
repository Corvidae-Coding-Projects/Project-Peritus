//! Stable error categories for qualification inputs and observations.

use thiserror::Error;

/// Failures returned while constructing or consuming qualification data.
#[derive(Debug, Error)]
pub enum QualificationError {
    /// A stable identifier did not satisfy the portable identifier grammar.
    #[error("invalid identifier `{value}`: {reason}")]
    InvalidIdentifier {
        /// Rejected identifier text.
        value: String,
        /// Stable explanation of the violated rule.
        reason: &'static str,
    },
    /// A digest was not lowercase hexadecimal SHA-256 text.
    #[error("invalid SHA-256 digest `{0}`")]
    InvalidDigest(String),
    /// A manifest artifact path was absolute or could escape its evidence root.
    #[error("invalid relative artifact path `{0}`")]
    InvalidArtifactPath(String),
    /// A numeric value violated a named bound.
    #[error("invalid value for {field}: {reason}")]
    InvalidValue {
        /// Stable field name.
        field: &'static str,
        /// Stable explanation of the violated rule.
        reason: &'static str,
    },
    /// Two entries declared the same stable key.
    #[error("duplicate {kind} `{id}`")]
    Duplicate {
        /// Entry category.
        kind: &'static str,
        /// Duplicated stable identifier.
        id: String,
    },
    /// A referenced workload or profile was not declared.
    #[error("unknown {kind} `{id}`")]
    UnknownReference {
        /// Referenced entry category.
        kind: &'static str,
        /// Missing stable identifier.
        id: String,
    },
    /// A workload cannot execute inside the selected resource envelope.
    #[error("workload `{workload}` exceeds profile resource `{resource}`")]
    WorkloadExceedsProfile {
        /// Workload identifier.
        workload: String,
        /// Resource whose declared bound is exceeded.
        resource: &'static str,
    },
    /// A measurement did not bind to the active qualification run.
    #[error(
        "measurement binding mismatch for {field}: expected `{expected}`, observed `{observed}`"
    )]
    MeasurementBinding {
        /// Binding field.
        field: &'static str,
        /// Required identifier.
        expected: String,
        /// Observed identifier.
        observed: String,
    },
    /// Measurement sequence or elapsed time moved backwards.
    #[error("non-monotonic measurement {field}: previous {previous}, observed {observed}")]
    NonMonotonicMeasurement {
        /// Monotonic field.
        field: &'static str,
        /// Previous value.
        previous: u64,
        /// Rejected value.
        observed: u64,
    },
    /// Measurement records were not contiguous in recorder order.
    #[error("measurement sequence mismatch: expected {expected}, observed {observed}")]
    MeasurementSequence {
        /// Required next sequence.
        expected: u64,
        /// Rejected sequence.
        observed: u64,
    },
    /// The measurement limit prevents unbounded ingestion.
    #[error("measurement limit {limit} exceeded")]
    MeasurementLimit {
        /// Configured maximum accepted records.
        limit: usize,
    },
    /// Resource accounting would violate a declared bound or lifecycle.
    #[error("resource accounting violation for {resource}: {reason}")]
    ResourceViolation {
        /// Resource or lifecycle name.
        resource: &'static str,
        /// Stable explanation of the violation.
        reason: &'static str,
    },
    /// Arithmetic could not be represented without weakening a bound.
    #[error("arithmetic overflow while accounting for {0}")]
    ArithmeticOverflow(&'static str),
    /// A JSON document could not be decoded.
    #[error("invalid {kind} JSON: {source}")]
    Json {
        /// Document category.
        kind: &'static str,
        /// Decoder source error.
        #[source]
        source: serde_json::Error,
    },
    /// A JSON-lines record could not be decoded.
    #[error("invalid measurement JSON on line {line}: {source}")]
    MeasurementJson {
        /// One-based line number.
        line: usize,
        /// Decoder source error.
        #[source]
        source: serde_json::Error,
    },
    /// A document exceeded its explicit byte limit.
    #[error("{kind} document exceeds byte limit {limit}")]
    DocumentLimit {
        /// Document category.
        kind: &'static str,
        /// Configured maximum bytes.
        limit: usize,
    },
    /// Serialization failed while producing reproducible evidence.
    #[error("could not serialize {kind}: {source}")]
    Serialization {
        /// Serialized value category.
        kind: &'static str,
        /// Encoder source error.
        #[source]
        source: serde_json::Error,
    },
}

impl QualificationError {
    pub(crate) const fn invalid_value(field: &'static str, reason: &'static str) -> Self {
        Self::InvalidValue { field, reason }
    }
}
