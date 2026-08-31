//! F0 harness-activation evidence corruption through the staged production daemon.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use rusqlite::Connection;

use crate::digest;
use crate::native_controller::args::{ControllerPaths, lower_sha256};

use super::process::{bounded_command, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-promotion-evidence-corruption-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd promotion evidence corruption failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "harness-promotion evidence corruption")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.record_sha256 == stage.corrupt_bytes_sha256
        || stage.bytes == 0
        || !stage.corruption_detected
    {
        return Err("harness-promotion checkpoint differs from the injected fault".into());
    }
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(stage.record_sha256),
        artifact_bytes: Some(stage.bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: output.status.to_string(),
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-promotion-evidence-corruption-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd promotion evidence containment failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "harness-promotion evidence containment")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    if recovered.evidence_id != stage.evidence_id
        || recovered.corrupt_bytes_sha256 != stage.corrupt_bytes_sha256
        || recovered.pointer_sha256 != stage.pointer_sha256
        || recovered.bytes != stage.bytes
        || recovered.committed_events != 16
        || recovered.aggregate_heads != 4
        || !recovered.verification.journal
        || !recovered.verification.promotion
        || !recovered.containment.corruption_detected
        || recovered.containment.mutation_admitted
    {
        return Err("harness-promotion evidence was not safely contained".into());
    }
    independently_verify(runtime, &recovered)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("promotion evidence journal is missing after recovery".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: true,
        pending_claims: 0,
        committed_events: Some(recovered.committed_events),
        aggregate_heads: Some(recovered.aggregate_heads),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(recovered.quarantine_sha256),
        artifact_bytes: Some(recovered.bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        lifecycle: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn independently_verify(
    runtime: &RuntimePaths,
    recovered: &RecoveryObservation,
) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::open(runtime.state.join("peritus.sqlite3"))?;
    let row: (Vec<u8>, Vec<u8>) = connection.query_row(
        "SELECT quarantine_digest, record_bytes FROM peritus_evidence_quarantine WHERE evidence_id = ?1",
        [decode_hex(&recovered.evidence_id)?.as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let quarantines: i64 =
        connection
            .query_row("SELECT COUNT(*) FROM peritus_evidence_quarantine", [], |row| row.get(0))?;
    let records: i64 =
        connection
            .query_row("SELECT COUNT(*) FROM peritus_evidence_records", [], |row| row.get(0))?;
    let unsettled: i64 =
        connection
            .query_row("SELECT COUNT(*) FROM outbox WHERE state != 3", [], |row| row.get(0))?;
    let pointer_rows: i64 = connection.query_row(
        "SELECT COUNT(*) FROM state_records WHERE namespace = 61442",
        [],
        |row| row.get(0),
    )?;
    if quarantines != 1
        || records != 2
        || unsettled != 0
        || pointer_rows != 1
        || row.0 != decode_hex(&recovered.quarantine_sha256)?
        || digest::hex(digest::bytes(&row.1)) != recovered.corrupt_bytes_sha256
        || row.1.len() as u64 != recovered.bytes
    {
        return Err("controller observed different promotion evidence state".into());
    }
    Ok(())
}

struct StageObservation {
    evidence_id: String,
    record_sha256: String,
    corrupt_bytes_sha256: String,
    pointer_sha256: String,
    bytes: u64,
    corruption_detected: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification promotion-evidence-corruption-stage ", 6)?;
    Ok(StageObservation {
        evidence_id: identity_field(values[0], "evidence_id")?,
        record_sha256: sha256_field(values[1], "record_sha256")?,
        corrupt_bytes_sha256: sha256_field(values[2], "corrupt_bytes_sha256")?,
        pointer_sha256: sha256_field(values[3], "pointer_sha256")?,
        bytes: number_field(values[4], "bytes")?,
        corruption_detected: boolean_field(values[5], "corruption_detected")?,
    })
}

struct RecoveryObservation {
    evidence_id: String,
    corrupt_bytes_sha256: String,
    quarantine_sha256: String,
    pointer_sha256: String,
    bytes: u64,
    committed_events: u64,
    aggregate_heads: u64,
    verification: VerificationObservation,
    containment: ContainmentObservation,
}

struct VerificationObservation {
    journal: bool,
    promotion: bool,
}

struct ContainmentObservation {
    corruption_detected: bool,
    mutation_admitted: bool,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification promotion-evidence-corruption-recover ", 11)?;
    Ok(RecoveryObservation {
        evidence_id: identity_field(values[0], "evidence_id")?,
        corrupt_bytes_sha256: sha256_field(values[1], "corrupt_bytes_sha256")?,
        quarantine_sha256: sha256_field(values[2], "quarantine_sha256")?,
        pointer_sha256: sha256_field(values[3], "pointer_sha256")?,
        bytes: number_field(values[4], "bytes")?,
        committed_events: number_field(values[5], "committed_events")?,
        aggregate_heads: number_field(values[6], "aggregate_heads")?,
        verification: VerificationObservation {
            journal: boolean_field(values[7], "journal_verified")?,
            promotion: boolean_field(values[8], "promotion_verified")?,
        },
        containment: ContainmentObservation {
            corruption_detected: boolean_field(values[9], "corruption_detected")?,
            mutation_admitted: boolean_field(values[10], "mutation_admitted")?,
        },
    })
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("unknown promotion evidence observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("promotion evidence observation field count differs".into());
    }
    Ok(values)
}

fn identity_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value.len() == 32 && value.bytes().all(is_lower_hex) {
        Ok(value.to_owned())
    } else {
        Err(format!("promotion evidence field {name} is not an identity").into())
    }
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if lower_sha256(value) {
        Ok(value.to_owned())
    } else {
        Err(format!("promotion evidence field {name} is not SHA-256").into())
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("promotion evidence field {name} is not boolean").into()),
    }
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed promotion evidence field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected promotion evidence field {name}, found {observed}").into());
    }
    Ok(value)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn hex_nibble(value: u8) -> Result<u8, Box<dyn std::error::Error>> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("promotion evidence contains non-hexadecimal bytes".into()),
    }
}

const fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
}
