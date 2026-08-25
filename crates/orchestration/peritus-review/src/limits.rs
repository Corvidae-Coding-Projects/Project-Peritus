//! Independently checked production bounds for D2 state and payloads.

use crate::error::{ReviewError, ReviewErrorKind, reject};

/// Fixed-point confidence in ten-thousandths, inclusive from zero to one.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Confidence(u16);

impl Confidence {
    /// Maximum exact confidence value (1.0000).
    pub const MAXIMUM: u16 = 10_000;

    /// Creates a checked fixed-point confidence.
    ///
    /// # Errors
    /// Returns [`ReviewErrorKind::InvalidInput`] above 1.0000.
    pub fn new(value: u16) -> Result<Self, ReviewError> {
        if value > Self::MAXIMUM {
            Err(reject(ReviewErrorKind::InvalidInput, "finding confidence exceeds 1.0000"))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the fixed-point value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    pub(super) const fn from_wire(value: u16) -> Self {
        Self(value)
    }
}

/// Complete independent limits for one review aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewLimits {
    cycles: u16,
    assignments: u16,
    submissions: u16,
    findings: u32,
    categories: u16,
    requirements: u16,
    locations: u16,
    evidence_references: u16,
    provenance_sources: u16,
    disposition_records: u16,
    path_bytes: u32,
    text_bytes: u32,
    opaque_bytes: u32,
    payload_bytes: u64,
    state_bytes: u64,
}

impl ReviewLimits {
    /// Compiled production ceilings. Lower caller values remain independently enforceable.
    pub const MAX_CYCLES: u16 = 1_024;
    /// Maximum assignment records.
    pub const MAX_ASSIGNMENTS: u16 = 1_024;
    /// Maximum structured submissions.
    pub const MAX_SUBMISSIONS: u16 = 1_024;
    /// Maximum retained findings.
    pub const MAX_FINDINGS: u32 = 65_535;
    /// Maximum categories per checked set.
    pub const MAX_CATEGORIES: u16 = 256;
    /// Maximum requirement identifiers per finding.
    pub const MAX_REQUIREMENTS: u16 = 1_024;
    /// Maximum locations per finding.
    pub const MAX_LOCATIONS: u16 = 1_024;
    /// Maximum evidence references per record.
    pub const MAX_EVIDENCE_REFERENCES: u16 = 4_096;
    /// Maximum retained provenance sources per finding.
    pub const MAX_PROVENANCE_SOURCES: u16 = 1_024;
    /// Maximum disposition records per finding.
    pub const MAX_DISPOSITION_RECORDS: u16 = 4_096;
    /// Maximum bytes in one repository-relative path.
    pub const MAX_PATH_BYTES: u32 = 4_096;
    /// Maximum bytes in one required text field.
    pub const MAX_TEXT_BYTES: u32 = 262_144;
    /// Maximum bytes in one inert opaque field.
    pub const MAX_OPAQUE_BYTES: u32 = 1_048_576;
    /// Maximum canonical bytes in one command payload.
    pub const MAX_PAYLOAD_BYTES: u64 = 16 * 1_048_576 - 16;
    /// Maximum complete aggregate bytes.
    pub const MAX_STATE_BYTES: u64 = 16 * 1_048_576 - 16;

    /// Validates every nonzero limit independently against its production ceiling.
    ///
    /// # Errors
    /// Returns [`ReviewErrorKind::InvalidLimit`] for the first zero or above-ceiling value.
    #[allow(clippy::too_many_arguments, reason = "independent security bounds stay explicit")]
    pub fn new(
        cycles: u16,
        assignments: u16,
        submissions: u16,
        findings: u32,
        categories: u16,
        requirements: u16,
        locations: u16,
        evidence_references: u16,
        provenance_sources: u16,
        disposition_records: u16,
        path_bytes: u32,
        text_bytes: u32,
        opaque_bytes: u32,
        payload_bytes: u64,
        state_bytes: u64,
    ) -> Result<Self, ReviewError> {
        let values = [
            (u64::from(cycles), u64::from(Self::MAX_CYCLES)),
            (u64::from(assignments), u64::from(Self::MAX_ASSIGNMENTS)),
            (u64::from(submissions), u64::from(Self::MAX_SUBMISSIONS)),
            (u64::from(findings), u64::from(Self::MAX_FINDINGS)),
            (u64::from(categories), u64::from(Self::MAX_CATEGORIES)),
            (u64::from(requirements), u64::from(Self::MAX_REQUIREMENTS)),
            (u64::from(locations), u64::from(Self::MAX_LOCATIONS)),
            (u64::from(evidence_references), u64::from(Self::MAX_EVIDENCE_REFERENCES)),
            (u64::from(provenance_sources), u64::from(Self::MAX_PROVENANCE_SOURCES)),
            (u64::from(disposition_records), u64::from(Self::MAX_DISPOSITION_RECORDS)),
            (u64::from(path_bytes), u64::from(Self::MAX_PATH_BYTES)),
            (u64::from(text_bytes), u64::from(Self::MAX_TEXT_BYTES)),
            (u64::from(opaque_bytes), u64::from(Self::MAX_OPAQUE_BYTES)),
            (payload_bytes, Self::MAX_PAYLOAD_BYTES),
            (state_bytes, Self::MAX_STATE_BYTES),
        ];
        if values.into_iter().any(|bound| bound.0 == 0 || bound.0 > bound.1) {
            return Err(reject(
                ReviewErrorKind::InvalidLimit,
                "review limit is zero or exceeds its production ceiling",
            ));
        }
        Ok(Self::from_wire(
            cycles,
            assignments,
            submissions,
            findings,
            categories,
            requirements,
            locations,
            evidence_references,
            provenance_sources,
            disposition_records,
            path_bytes,
            text_bytes,
            opaque_bytes,
            payload_bytes,
            state_bytes,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        cycles: u16,
        assignments: u16,
        submissions: u16,
        findings: u32,
        categories: u16,
        requirements: u16,
        locations: u16,
        evidence_references: u16,
        provenance_sources: u16,
        disposition_records: u16,
        path_bytes: u32,
        text_bytes: u32,
        opaque_bytes: u32,
        payload_bytes: u64,
        state_bytes: u64,
    ) -> Self {
        Self {
            cycles,
            assignments,
            submissions,
            findings,
            categories,
            requirements,
            locations,
            evidence_references,
            provenance_sources,
            disposition_records,
            path_bytes,
            text_bytes,
            opaque_bytes,
            payload_bytes,
            state_bytes,
        }
    }

    /// Maximum retained cycles.
    #[must_use]
    pub const fn cycles(self) -> u16 {
        self.cycles
    }
    /// Maximum assignments.
    #[must_use]
    pub const fn assignments(self) -> u16 {
        self.assignments
    }
    /// Maximum submissions.
    #[must_use]
    pub const fn submissions(self) -> u16 {
        self.submissions
    }
    /// Maximum findings.
    #[must_use]
    pub const fn findings(self) -> u32 {
        self.findings
    }
    /// Maximum categories per set.
    #[must_use]
    pub const fn categories(self) -> u16 {
        self.categories
    }
    /// Maximum requirement identifiers.
    #[must_use]
    pub const fn requirements(self) -> u16 {
        self.requirements
    }
    /// Maximum locations.
    #[must_use]
    pub const fn locations(self) -> u16 {
        self.locations
    }
    /// Maximum evidence references.
    #[must_use]
    pub const fn evidence_references(self) -> u16 {
        self.evidence_references
    }
    /// Maximum provenance sources.
    #[must_use]
    pub const fn provenance_sources(self) -> u16 {
        self.provenance_sources
    }
    /// Maximum disposition records.
    #[must_use]
    pub const fn disposition_records(self) -> u16 {
        self.disposition_records
    }
    /// Maximum repository-relative path bytes.
    #[must_use]
    pub const fn path_bytes(self) -> u32 {
        self.path_bytes
    }
    /// Maximum required text bytes.
    #[must_use]
    pub const fn text_bytes(self) -> u32 {
        self.text_bytes
    }
    /// Maximum opaque bytes.
    #[must_use]
    pub const fn opaque_bytes(self) -> u32 {
        self.opaque_bytes
    }
    /// Maximum command payload bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> u64 {
        self.payload_bytes
    }
    /// Maximum complete state bytes.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }
}
