//! Canonical native-host partitioning and aggregation for H0.

use std::collections::BTreeMap;

use crate::{
    CaseReport, IntegratedCandidate, ProbeId, ProbeSpec, ProbeTarget, QualificationError,
    QualificationLimits, QualificationRun, error::protocol,
};

/// Native platform responsible for one canonical H0 campaign shard.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationPlatform {
    /// Runs portable tier-one probes and the Linux-specific backend probe.
    Linux,
    /// Runs the macOS-specific backend probe.
    Macos,
    /// Runs the Windows-specific backend probe.
    Windows,
}

impl QualificationPlatform {
    /// Complete canonical platform order used by aggregation.
    pub const ALL: [Self; 3] = [Self::Linux, Self::Macos, Self::Windows];

    /// Returns the stable platform code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }

    /// Returns the supported platform on which this binary is executing.
    #[must_use]
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }

    pub(crate) const fn owns(self, target: ProbeTarget) -> bool {
        match self {
            Self::Linux => matches!(target, ProbeTarget::TierOneHost | ProbeTarget::Linux),
            Self::Macos => matches!(target, ProbeTarget::Macos),
            Self::Windows => matches!(target, ProbeTarget::Windows),
        }
    }
}

/// One candidate-bound subset executed on exactly one native platform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationShard {
    candidate: IntegratedCandidate,
    limits: QualificationLimits,
    platform: QualificationPlatform,
    cases: Vec<CaseReport>,
}

impl QualificationShard {
    pub(crate) fn new(
        candidate: IntegratedCandidate,
        limits: QualificationLimits,
        platform: QualificationPlatform,
        cases: Vec<CaseReport>,
    ) -> Result<Self, QualificationError> {
        let expected =
            ProbeSpec::h0_production().iter().filter(|spec| platform.owns(spec.target()));
        if cases.iter().map(CaseReport::spec).ne(expected.copied()) {
            return Err(protocol("H0 shard does not contain its canonical platform probes"));
        }
        Ok(Self { candidate, limits, platform, cases })
    }

    /// Returns the exact integrated candidate.
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }

    /// Returns the per-case limits used by this shard.
    #[must_use]
    pub const fn limits(&self) -> QualificationLimits {
        self.limits
    }

    /// Returns the native platform that produced this shard.
    #[must_use]
    pub const fn platform(&self) -> QualificationPlatform {
        self.platform
    }

    /// Borrows case reports in canonical catalog order.
    #[must_use]
    pub fn cases(&self) -> &[CaseReport] {
        &self.cases
    }

    /// Encodes this shard as bounded, deterministic JSON for cross-host aggregation.
    ///
    /// # Errors
    ///
    /// Returns when JSON serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        crate::interchange::encode(self)
    }

    /// Parses only a complete passing shard suitable for aggregation.
    ///
    /// Failed shard documents remain useful diagnostics but cannot cross this admission boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed, oversized, failed, stale, incomplete, or internally inconsistent input.
    pub fn parse_ready_json(bytes: &[u8]) -> Result<Self, QualificationError> {
        crate::interchange::decode_ready(bytes)
    }
}

pub fn aggregate(
    mut shards: Vec<QualificationShard>,
) -> Result<QualificationRun, QualificationError> {
    shards.sort_by_key(QualificationShard::platform);
    if shards.len() != QualificationPlatform::ALL.len()
        || shards.iter().map(QualificationShard::platform).ne(QualificationPlatform::ALL)
    {
        return Err(protocol(
            "H0 aggregation requires exactly one Linux, macOS, and Windows shard",
        ));
    }
    let candidate = shards[0].candidate;
    let limits = shards[0].limits;
    if shards.iter().any(|shard| shard.candidate != candidate || shard.limits != limits) {
        return Err(protocol("H0 shards do not bind the same candidate and limits"));
    }

    let mut by_probe = BTreeMap::<ProbeId, CaseReport>::new();
    for case in shards.into_iter().flat_map(|shard| shard.cases) {
        if by_probe.insert(case.spec().id(), case).is_some() {
            return Err(protocol("H0 shards contain a duplicate probe report"));
        }
    }
    let cases = ProbeSpec::h0_production()
        .iter()
        .map(|spec| {
            by_probe
                .remove(&spec.id())
                .ok_or_else(|| protocol("H0 shards omitted a production probe report"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    QualificationRun::new(candidate, limits, cases)
}
