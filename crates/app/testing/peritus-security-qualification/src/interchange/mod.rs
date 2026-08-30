//! Durable, candidate-bound interchange for native H0 shards.

mod candidate;
mod case;

use serde::{Deserialize, Serialize};

use crate::{
    CaseOutcome, ProbeSpec, QualificationError, QualificationErrorCode, QualificationLimits,
    QualificationPlatform, QualificationRecovery, QualificationShard,
};

use self::candidate::CandidateDocument;
use self::case::CaseDocument;

const SCHEMA_VERSION: u8 = 1;
const MAX_SHARD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShardDocument {
    schema_version: u8,
    platform: String,
    candidate: CandidateDocument,
    limits: LimitsDocument,
    status: String,
    cases: Vec<CaseDocument>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsDocument {
    duration_millis: u64,
    processes: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifacts: u32,
}

pub fn encode(shard: &QualificationShard) -> Result<Vec<u8>, QualificationError> {
    let limits = shard.limits();
    let document = ShardDocument {
        schema_version: SCHEMA_VERSION,
        platform: shard.platform().as_str().to_owned(),
        candidate: CandidateDocument::from_candidate(shard.candidate()),
        limits: LimitsDocument {
            duration_millis: limits.max_duration_millis(),
            processes: limits.max_processes(),
            peak_memory_bytes: limits.max_peak_memory_bytes(),
            output_bytes: limits.max_output_bytes(),
            artifacts: limits.max_artifacts(),
        },
        status: if shard.cases().iter().all(|case| case.outcome() == CaseOutcome::Passed) {
            "passed"
        } else {
            "failed"
        }
        .to_owned(),
        cases: shard.cases().iter().map(CaseDocument::from_case).collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| interchange(format!("encode shard JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode_ready(bytes: &[u8]) -> Result<QualificationShard, QualificationError> {
    if bytes.is_empty() || bytes.len() > MAX_SHARD_BYTES {
        return Err(interchange("H0 shard JSON is empty or exceeds its byte bound"));
    }
    let document: ShardDocument = serde_json::from_slice(bytes)
        .map_err(|error| interchange(format!("decode shard JSON: {error}")))?;
    if document.schema_version != SCHEMA_VERSION || document.status != "passed" {
        return Err(interchange("H0 aggregation accepts only a passed shard schema v1 document"));
    }
    let platform = parse_platform(&document.platform)?;
    let candidate = document.candidate.into_candidate()?;
    let limits = QualificationLimits::new(
        document.limits.duration_millis,
        document.limits.processes,
        document.limits.peak_memory_bytes,
        document.limits.output_bytes,
        document.limits.artifacts,
    )?;
    let expected = ProbeSpec::h0_production()
        .iter()
        .copied()
        .filter(|spec| platform.owns(spec.target()))
        .collect::<Vec<_>>();
    if document.cases.len() != expected.len() {
        return Err(interchange("H0 shard case count does not match its native platform"));
    }
    let cases = document
        .cases
        .into_iter()
        .zip(expected)
        .map(|(case, spec)| case.into_ready(candidate, spec, limits))
        .collect::<Result<Vec<_>, _>>()?;
    QualificationShard::new(candidate, limits, platform, cases)
}

fn parse_platform(value: &str) -> Result<QualificationPlatform, QualificationError> {
    match value {
        "linux" => Ok(QualificationPlatform::Linux),
        "macos" => Ok(QualificationPlatform::Macos),
        "windows" => Ok(QualificationPlatform::Windows),
        _ => Err(interchange("H0 shard platform is not a canonical tier-one platform")),
    }
}

fn interchange(detail: impl Into<String>) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Manifest,
        QualificationRecovery::Quarantine,
        "validate native H0 shard interchange",
        detail,
    )
}
