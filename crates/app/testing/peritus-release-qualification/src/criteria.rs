//! Exact AC-01 through AC-25 production acceptance catalog and evidence mapping.

use serde::Serialize;

use peritus_release_artifacts::{Sha256Digest, digest_bytes};

use crate::{EvidenceReference, QualificationError, QualificationErrorCode};

/// Number of production acceptance criteria in the architecture contract.
pub const ACCEPTANCE_CRITERIA_COUNT: usize = 25;

/// Exact production acceptance criterion identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceptanceCriterion {
    /// AC-01: clean cross-platform ordinary Rust and end-to-end suites.
    Ac01,
    /// AC-02: clean locked Verus verification and production build.
    Ac02,
    /// AC-03: complete deterministic proof-obligation inventory.
    Ac03,
    /// AC-04: trusted-construct allowlist and compensating evidence.
    Ac04,
    /// AC-05: privileged values constructible only through verified transitions.
    Ac05,
    /// AC-06: every illegal lifecycle edge rejected consistently.
    Ac06,
    /// AC-07: power-loss recovery at every durable commit point.
    Ac07,
    /// AC-08: byte-for-byte deterministic replay from empty projections.
    Ac08,
    /// AC-09: complete malicious-repository attack suite.
    Ac09,
    /// AC-10: tier-one sandbox conformance and independent escape review.
    Ac10,
    /// AC-11: writer, reviewer, and fixer isolation.
    Ac11,
    /// AC-12: candidate mutation invalidates stale evidence.
    Ac12,
    /// AC-13: exhaustion is non-success with complete evidence.
    Ac13,
    /// AC-14: daemon restart reconciles every lifecycle phase.
    Ac14,
    /// AC-15: complete provider interruption and retry contracts.
    Ac15,
    /// AC-16: all historical migrations, corruption rejection, and portable export.
    Ac16,
    /// AC-17: evolution candidates cannot escape sealed authority.
    Ac17,
    /// AC-18: gated promotion and atomic history-preserving rollback.
    Ac18,
    /// AC-19: causal observability, failure taxonomy, and secret redaction.
    Ac19,
    /// AC-20: documented load and soak service-level objectives.
    Ac20,
    /// AC-21: public command/protocol documentation and end-to-end coverage.
    Ac21,
    /// AC-22: architecture dependency and public-surface checks.
    Ac22,
    /// AC-23: independent representative multi-language campaign.
    Ac23,
    /// AC-24: reproducible signed release supply-chain bundle and review.
    Ac24,
    /// AC-25: no quarantines, blockers, undocumented unsafe, or placeholders.
    Ac25,
}

impl AcceptanceCriterion {
    /// Returns every criterion in authoritative numeric order.
    #[must_use]
    pub const fn all() -> [Self; ACCEPTANCE_CRITERIA_COUNT] {
        [
            Self::Ac01,
            Self::Ac02,
            Self::Ac03,
            Self::Ac04,
            Self::Ac05,
            Self::Ac06,
            Self::Ac07,
            Self::Ac08,
            Self::Ac09,
            Self::Ac10,
            Self::Ac11,
            Self::Ac12,
            Self::Ac13,
            Self::Ac14,
            Self::Ac15,
            Self::Ac16,
            Self::Ac17,
            Self::Ac18,
            Self::Ac19,
            Self::Ac20,
            Self::Ac21,
            Self::Ac22,
            Self::Ac23,
            Self::Ac24,
            Self::Ac25,
        ]
    }

    /// Returns the one-based architecture criterion number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8 + 1
    }

    /// Returns the stable `AC-NN` identifier.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Ac01 => "AC-01",
            Self::Ac02 => "AC-02",
            Self::Ac03 => "AC-03",
            Self::Ac04 => "AC-04",
            Self::Ac05 => "AC-05",
            Self::Ac06 => "AC-06",
            Self::Ac07 => "AC-07",
            Self::Ac08 => "AC-08",
            Self::Ac09 => "AC-09",
            Self::Ac10 => "AC-10",
            Self::Ac11 => "AC-11",
            Self::Ac12 => "AC-12",
            Self::Ac13 => "AC-13",
            Self::Ac14 => "AC-14",
            Self::Ac15 => "AC-15",
            Self::Ac16 => "AC-16",
            Self::Ac17 => "AC-17",
            Self::Ac18 => "AC-18",
            Self::Ac19 => "AC-19",
            Self::Ac20 => "AC-20",
            Self::Ac21 => "AC-21",
            Self::Ac22 => "AC-22",
            Self::Ac23 => "AC-23",
            Self::Ac24 => "AC-24",
            Self::Ac25 => "AC-25",
        }
    }

    /// Returns the authoritative architecture meaning without weakening or substituting it.
    #[must_use]
    pub const fn statement(self) -> &'static str {
        match self {
            Self::Ac01 => {
                "A clean checkout passes formatting, strict Clippy, unit, integration, documentation, compatibility, property, concurrency, Miri-eligible, fuzz smoke, security, and end-to-end suites on all tier-one platforms."
            }
            Self::Ac02 => {
                "cargo verus verify --workspace and cargo verus build --release succeed from a clean locked dependency graph with no unapproved trusted construct."
            }
            Self::Ac03 => {
                "The proof obligation inventory reports every deterministic decision function as verified or records an approved, narrowly scoped exclusion with compensating evidence."
            }
            Self::Ac04 => {
                "Machine checks show no assume, admit, axiom, external_body, or equivalent outside the trust-boundary allowlist; every allowlisted entry is linked to a threat analysis and refinement test."
            }
            Self::Ac05 => {
                "Model, tool, and ordinary Rust callers cannot construct privileged tokens, accepted states, closed findings, current evidence, or promoted harness states without going through verified transitions."
            }
            Self::Ac06 => {
                "A recorded state-machine test suite attempts every illegal lifecycle edge and proves or rejects it consistently in Verus, property tests, and protocol conformance tests."
            }
            Self::Ac07 => {
                "Power-loss and crash injection at every journal, blob, snapshot, lease, patch, gate, and promotion commit point recovers to the documented state without journal divergence or false success."
            }
            Self::Ac08 => {
                "Deterministic replay from an empty projection database reproduces authoritative state and all acceptance decisions byte-for-byte for the compatibility corpus."
            }
            Self::Ac09 => {
                "A malicious-repository suite covers traversal, symlink races, submodule and worktree tricks, case-insensitive aliases, device paths, shell injection, poisoned instructions, oversized output, terminal escapes, and secret-exfiltration attempts."
            }
            Self::Ac10 => {
                "Each tier-one sandbox passes the common capability conformance suite and an independent escape-focused security review."
            }
            Self::Ac11 => {
                "Writer, reviewer, and fixer isolation tests prove that read-only actors cannot mutate and that writable actors cannot approve or waive their own results."
            }
            Self::Ac12 => {
                "Any candidate mutation invalidates prior gate and review evidence; stale evidence cannot be used to accept a new revision."
            }
            Self::Ac13 => {
                "Budget or retry exhaustion produces a non-success terminal state and a complete evidence bundle; no timeout path marks work accepted."
            }
            Self::Ac14 => {
                "A daemon killed and restarted during every active lifecycle phase resumes, reconciles, or explicitly fails owned tasks without orphaned authoritative work."
            }
            Self::Ac15 => {
                "Provider contract tests cover streaming interruption, duplicated events, out-of-order chunks, retry-after, malformed structured output, partial tool calls, cancellation, and idempotent retry."
            }
            Self::Ac16 => {
                "The event store migrates every historical schema fixture forward, rejects corrupt or hash-divergent journals, and can export a portable evidence bundle."
            }
            Self::Ac17 => {
                "Evolution red-team tests demonstrate that candidates cannot read sealed answers, edit evaluators, change model or resource profiles, bypass safety policy, or promote themselves."
            }
            Self::Ac18 => {
                "Promotion requires all configured statistical, correctness, safety, resource, and authority gates against immutable candidate and baseline revisions; rollback is atomic and preserves both histories."
            }
            Self::Ac19 => {
                "Observability reports cite source event and artifact IDs, distinguish infrastructure failure from task failure, and redact seeded secrets from default logs and exported evidence."
            }
            Self::Ac20 => {
                "Load and soak tests meet documented service-level objectives for concurrent runs, event append latency, terminal streaming, memory bounds, cancellation latency, and recovery time."
            }
            Self::Ac21 => {
                "Every public command and protocol method has reference documentation, examples, stable error codes, and end-to-end tests."
            }
            Self::Ac22 => {
                "Architecture checks report no dependency cycles, forbidden upward dependencies, god root modules, unowned generated files, or public API leakage of implementation crates."
            }
            Self::Ac23 => {
                "The final independent writer, reviewer, and fixer campaign completes representative Rust, TypeScript, Python, Java, and mixed-repository tasks with reproducible evidence and no manual state repair."
            }
            Self::Ac24 => {
                "Release artifacts are reproducible, signed, accompanied by SBOM and provenance, license notices, migration and recovery documentation, and a completed security review."
            }
            Self::Ac25 => {
                "There are no quarantined tests, ignored failing tests, unresolved release-blocking findings, undocumented unsafe blocks, or placeholder production implementations."
            }
        }
    }
}

/// Nonempty evidence references supporting one acceptance criterion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CriterionMapping {
    criterion: AcceptanceCriterion,
    statement: &'static str,
    evidence: Vec<EvidenceReference>,
}

impl CriterionMapping {
    /// Creates a mapping and removes repeated identical evidence references.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when no evidence reference is supplied or more than 128
    /// references are supplied.
    pub fn new(
        criterion: AcceptanceCriterion,
        mut evidence: Vec<EvidenceReference>,
    ) -> Result<Self, QualificationError> {
        evidence.sort_by(|left, right| {
            left.kind()
                .cmp(&right.kind())
                .then(left.path().cmp(right.path()))
                .then(left.payload_digest().cmp(&right.payload_digest()))
        });
        evidence.dedup();
        if evidence.is_empty() || evidence.len() > 128 {
            return Err(QualificationError::new(
                QualificationErrorCode::MissingEvidence,
                "map acceptance criterion",
                format!("{} must reference 1 through 128 evidence records", criterion.id()),
            ));
        }
        Ok(Self { criterion, statement: criterion.statement(), evidence })
    }

    /// Returns the mapped criterion.
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion {
        self.criterion
    }

    /// Returns evidence references in canonical order.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceReference] {
        &self.evidence
    }
}

/// Complete evidence map containing each acceptance criterion exactly once.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CriterionEvidenceMap {
    schema_version: u32,
    mappings: Vec<CriterionMapping>,
}

impl CriterionEvidenceMap {
    /// Validates exact AC-01 through AC-25 coverage.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when a criterion is missing or duplicated.
    pub fn new(mut mappings: Vec<CriterionMapping>) -> Result<Self, QualificationError> {
        mappings.sort_by_key(CriterionMapping::criterion);
        for criterion in AcceptanceCriterion::all() {
            let count = mappings.iter().filter(|mapping| mapping.criterion == criterion).count();
            if count != 1 {
                return Err(QualificationError::new(
                    if count == 0 {
                        QualificationErrorCode::MissingEvidence
                    } else {
                        QualificationErrorCode::Duplicate
                    },
                    "create criterion evidence map",
                    format!("{} must be mapped exactly once", criterion.id()),
                ));
            }
        }
        Ok(Self { schema_version: 1, mappings })
    }

    /// Returns all 25 mappings in numeric order.
    #[must_use]
    pub fn mappings(&self) -> &[CriterionMapping] {
        &self.mappings
    }

    /// Returns a deterministic digest of the complete map.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, QualificationError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }

    /// Serializes deterministic compact criterion-map JSON.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self).map_err(|source| {
            QualificationError::serialization("serialize criterion evidence map", source)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ACCEPTANCE_CRITERIA_COUNT, AcceptanceCriterion, CriterionEvidenceMap};

    #[test]
    fn catalog_is_exactly_twenty_five_and_stably_numbered() {
        let catalog = AcceptanceCriterion::all();
        assert_eq!(catalog.len(), ACCEPTANCE_CRITERIA_COUNT);
        for (index, criterion) in catalog.iter().enumerate() {
            assert_eq!(usize::from(criterion.number()), index + 1);
        }
    }

    #[test]
    fn empty_mapping_set_fails_closed() {
        assert!(CriterionEvidenceMap::new(Vec::new()).is_err());
    }
}
