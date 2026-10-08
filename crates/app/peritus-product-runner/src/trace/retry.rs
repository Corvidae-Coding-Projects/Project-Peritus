//! Versioned retry intent, schedule, settlement, and restart reconstruction.

use peritus_agent::{
    DeveloperLoopError, DeveloperRetryDisposition, DeveloperRetryRecord, DeveloperRetryRecovery,
};
use peritus_model_protocol::{ModelRequest, OutcomeCertainty};
use peritus_types::{ProviderProfileId, Sha256Digest};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read as _, Seek as _, Write as _},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u16 = 3;
const MIN_SCHEMA_VERSION: u16 = 2;
const FRONTIER_SCHEMA_VERSION: u16 = 1;
// Retry records contain only fixed-width identities, scalar timing, and closed-enum strings.
// This per-record representation bound prevents malformed allocation without limiting trace life.
const MAX_RETRY_RECORD_BYTES: u64 = 4 * 1024;
const MAX_FRONTIER_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ScheduleRecord {
    schema_version: u16,
    turn: u16,
    attempt: u64,
    request_id_sha256: [u8; 32],
    request_fingerprint_sha256: [u8; 32],
    provider_profile: [u8; 16],
    native_session_sha256: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_selection_sha256: Option<[u8; 32]>,
    certainty: String,
    max_attempts: Option<u64>,
    elapsed_millis: u64,
    delay_millis: u64,
    next_eligible_unix_millis: u64,
    retry_after_millis: Option<u64>,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyScheduleRecord {
    turn: u16,
    attempt: u8,
    max_attempts: u8,
    #[serde(rename = "elapsed_millis")]
    _elapsed_millis: u64,
    delay_millis: u64,
    retry_after_millis: Option<u64>,
    reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct BoundaryRecord {
    schema_version: u16,
    turn: u16,
    attempt: u64,
    request_id_sha256: [u8; 32],
    request_fingerprint_sha256: [u8; 32],
    provider_profile: [u8; 16],
    native_session_sha256: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    superseded_by_provider_selection_sha256: Option<[u8; 32]>,
    disposition: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Latest {
    None,
    Scheduled(ScheduleRecord),
    Admitted(BoundaryRecord),
    Settled(BoundaryRecord),
    ReconciliationRequired(BoundaryRecord),
    Superseded(BoundaryRecord),
}

// The frontier is derived acceleration, never independent dispatch authority. Its state is usable
// only after the referenced retry frame is read from the immutable trace and matches byte-for-byte;
// recovery then scans every frame after that anchor, so a stale publication cannot hide a boundary.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FrontierRecord {
    schema_version: u16,
    anchor_offset: u64,
    anchor_end: u64,
    frame_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrameAnchor {
    offset: u64,
    end: u64,
    frame_sha256: [u8; 32],
}

struct Scan {
    latest: Latest,
}

struct RecoveredFrontier {
    latest: Latest,
    anchor: FrameAnchor,
}

pub(crate) struct PendingRetry {
    turn: u16,
    attempt: u64,
    request_id_sha256: [u8; 32],
}

impl PendingRetry {
    pub(crate) const fn turn(&self) -> u16 {
        self.turn
    }

    pub(crate) const fn attempt(&self) -> u64 {
        self.attempt
    }

    pub(crate) const fn request_id_sha256(&self) -> [u8; 32] {
        self.request_id_sha256
    }
}

pub(super) fn schedule_payload(
    record: &DeveloperRetryRecord,
) -> Result<Vec<u8>, DeveloperLoopError> {
    if record.certainty() != OutcomeCertainty::DefinitelyNotAccepted {
        return Err(failure("unsafe retry schedule certainty"));
    }
    serde_json::to_vec(&ScheduleRecord {
        schema_version: SCHEMA_VERSION,
        turn: record.turn(),
        attempt: record.attempt(),
        request_id_sha256: record.request_id_digest().into_bytes(),
        request_fingerprint_sha256: record.request_fingerprint().into_bytes(),
        provider_profile: record.provider_profile_id().into_bytes(),
        native_session_sha256: record.native_session_digest().map(Sha256Digest::into_bytes),
        provider_selection_sha256: record
            .provider_selection_digest()
            .map(Sha256Digest::into_bytes),
        certainty: "definitely_not_accepted".to_owned(),
        max_attempts: None,
        elapsed_millis: record.elapsed_millis(),
        delay_millis: record.delay_millis(),
        next_eligible_unix_millis: record.next_eligible_unix_millis(),
        retry_after_millis: record.retry_after_millis(),
        reason: record.reason().as_str().to_owned(),
    })
    .map_err(|_| failure("encode retry schedule"))
}

pub(super) fn attempt_payload(
    turn: u16,
    attempt: u64,
    request: &ModelRequest,
) -> Result<Vec<u8>, DeveloperLoopError> {
    boundary_payload(turn, attempt, request, "admitted")
}

pub(super) fn disposition_payload(
    turn: u16,
    attempt: u64,
    request: &ModelRequest,
    disposition: DeveloperRetryDisposition,
) -> Result<Vec<u8>, DeveloperLoopError> {
    boundary_payload(
        turn,
        attempt,
        request,
        match disposition {
            DeveloperRetryDisposition::Settled => "settled",
            DeveloperRetryDisposition::ReconciliationRequired => "reconciliation_required",
        },
    )
}

fn superseded_payload(
    record: &ScheduleRecord,
    provider_selection: Option<Sha256Digest>,
) -> Result<Vec<u8>, DeveloperLoopError> {
    serde_json::to_vec(&BoundaryRecord {
        schema_version: SCHEMA_VERSION,
        turn: record.turn,
        attempt: record.attempt,
        request_id_sha256: record.request_id_sha256,
        request_fingerprint_sha256: record.request_fingerprint_sha256,
        provider_profile: record.provider_profile,
        native_session_sha256: record.native_session_sha256,
        superseded_by_provider_selection_sha256: provider_selection
            .map(Sha256Digest::into_bytes),
        disposition: "superseded".to_owned(),
    })
    .map_err(|_| failure("encode retry supersession"))
}

fn boundary_payload(
    turn: u16,
    attempt: u64,
    request: &ModelRequest,
    disposition: &str,
) -> Result<Vec<u8>, DeveloperLoopError> {
    serde_json::to_vec(&BoundaryRecord {
        schema_version: SCHEMA_VERSION,
        turn,
        attempt,
        request_id_sha256: request_digest(request),
        request_fingerprint_sha256: request.fingerprint()?.digest().into_bytes(),
        provider_profile: request.profile_id().into_bytes(),
        native_session_sha256: native_session_digest(request),
        superseded_by_provider_selection_sha256: None,
        disposition: disposition.to_owned(),
    })
    .map_err(|_| failure("encode retry boundary"))
}

pub(super) fn recover(
    path: &Path,
    request_prefix: &str,
    turn: u16,
    provider_profile_id: ProviderProfileId,
    expected_native_session: Option<Sha256Digest>,
    expected_request_fingerprint: Sha256Digest,
) -> Result<Option<DeveloperRetryRecovery>, DeveloperLoopError> {
    let latest = scan(path)?;
    match latest {
        Latest::None => Ok(None),
        Latest::Scheduled(record) => {
            if !schedule_matches(
                &record,
                request_prefix,
                turn,
                provider_profile_id,
                expected_native_session,
                expected_request_fingerprint,
            )? {
                return Err(DeveloperLoopError::RecoveryRequired(
                    "a safe retry is pending for a prior logical request; reopen that exact request before continuing"
                        .to_owned(),
                ));
            }
            Ok(Some(DeveloperRetryRecovery::new(
                record
                    .attempt
                    .checked_add(1)
                    .ok_or_else(|| failure("retry attempt identity overflow"))?,
                record.next_eligible_unix_millis,
                record.elapsed_millis,
                record
                    .next_eligible_unix_millis
                    .checked_sub(record.delay_millis)
                    .ok_or_else(|| failure("retry schedule eligibility precedes its delay"))?,
            )))
        }
        Latest::Settled(record) => {
            if boundary_matches(
                &record,
                request_prefix,
                turn,
                provider_profile_id,
                expected_native_session,
                expected_request_fingerprint,
            )? {
                return Err(DeveloperLoopError::RecoveryRequired(
                    "this logical provider turn is already settled and must be recovered instead of resent"
                        .to_owned(),
                ));
            }
            Ok(None)
        }
        Latest::Superseded(record) => {
            if boundary_matches(
                &record,
                request_prefix,
                turn,
                provider_profile_id,
                expected_native_session,
                expected_request_fingerprint,
            )? {
                return Err(DeveloperLoopError::RecoveryRequired(
                    "this stale retry was superseded by newer governing input and must not be resent"
                        .to_owned(),
                ));
            }
            Ok(None)
        }
        Latest::Admitted(_) | Latest::ReconciliationRequired(_) => Err(
            DeveloperLoopError::RecoveryRequired(
                "provider request acceptance is unresolved; reconcile the retained native request before continuing"
                    .to_owned(),
            ),
        ),
    }
}

pub(super) fn pending(path: &Path) -> Result<Option<PendingRetry>, DeveloperLoopError> {
    match scan(path)? {
        Latest::Scheduled(record) => Ok(Some(PendingRetry {
            turn: record.turn,
            attempt: record.attempt,
            request_id_sha256: record.request_id_sha256,
        })),
        Latest::None | Latest::Settled(_) | Latest::Superseded(_) => Ok(None),
        Latest::Admitted(_) | Latest::ReconciliationRequired(_) => Err(
            DeveloperLoopError::RecoveryRequired(
                "provider request acceptance is unresolved; reconcile the retained native request before continuing"
                    .to_owned(),
            ),
        ),
    }
}

pub(super) fn append(
    path: &Path,
    kind: super::DeveloperTraceFrameKind,
    payload: &[u8],
) -> Result<(), DeveloperLoopError> {
    if !is_retry_kind(kind) {
        return Err(failure("retry frontier received an unrelated frame"));
    }
    let mut file = super::open(path).map_err(|_| failure("open retry trace for append"))?;
    file.lock().map_err(|_| failure("lock retry trace for append"))?;
    let scan = scan_file(path, &mut file)?;
    append_transition(path, &mut file, scan.latest, kind, payload)
}

pub(super) fn supersede(
    path: &Path,
    request_prefix: &str,
    turn: u16,
    scheduled_attempt: u64,
) -> Result<(), DeveloperLoopError> {
    let mut file = super::open(path).map_err(|_| failure("open retry trace for supersession"))?;
    file.lock().map_err(|_| failure("lock retry trace for supersession"))?;
    let scan = scan_file(path, &mut file)?;
    let payload = match &scan.latest {
        Latest::Scheduled(record)
            if record.turn == turn
                && record.attempt == scheduled_attempt
                && record.request_id_sha256
                    == expected_digest(request_prefix, turn, scheduled_attempt).into_bytes() =>
        {
            superseded_payload(record, None)?
        }
        Latest::Scheduled(_) => {
            return Err(failure("retry supersession identity does not match the safe schedule"));
        }
        Latest::Admitted(_) | Latest::ReconciliationRequired(_) => {
            return Err(DeveloperLoopError::RecoveryRequired(
                "an admitted or ambiguous provider request cannot be superseded; reconcile it before continuing"
                    .to_owned(),
            ));
        }
        Latest::None | Latest::Settled(_) | Latest::Superseded(_) => {
            return Err(failure("retry supersession has no pending safe schedule"));
        }
    };
    append_transition(
        path,
        &mut file,
        scan.latest,
        super::DeveloperTraceFrameKind::RetryDisposition,
        &payload,
    )
}

pub(super) fn supersede_for_provider_selection(
    path: &Path,
    request_prefix: &str,
    turn: u16,
    current_selection: Sha256Digest,
) -> Result<bool, DeveloperLoopError> {
    let mut file = super::open(path)
        .map_err(|_| failure("open retry trace for provider selection supersession"))?;
    file.lock()
        .map_err(|_| failure("lock retry trace for provider selection supersession"))?;
    let scan = scan_file(path, &mut file)?;
    let payload = match &scan.latest {
        Latest::Scheduled(record)
            if record.turn == turn
                && record.request_id_sha256
                    == expected_digest(request_prefix, turn, record.attempt).into_bytes()
                && record
                    .provider_selection_sha256
                    .is_some_and(|prior| prior != current_selection.into_bytes()) =>
        {
            Some(superseded_payload(record, Some(current_selection))?)
        }
        Latest::Admitted(_) | Latest::ReconciliationRequired(_) => {
            return Err(DeveloperLoopError::RecoveryRequired(
                "an admitted or ambiguous provider request cannot be superseded by a provider selection; reconcile it before continuing"
                    .to_owned(),
            ));
        }
        Latest::Superseded(record)
            if record.turn == turn
                && record.request_id_sha256
                    == expected_digest(request_prefix, turn, record.attempt).into_bytes()
                && record.superseded_by_provider_selection_sha256.is_some() =>
        {
            return Ok(true);
        }
        Latest::None | Latest::Scheduled(_) | Latest::Settled(_) | Latest::Superseded(_) => None,
    };
    let Some(payload) = payload else { return Ok(false) };
    append_transition(
        path,
        &mut file,
        scan.latest,
        super::DeveloperTraceFrameKind::RetryDisposition,
        &payload,
    )?;
    Ok(true)
}

fn append_transition(
    path: &Path,
    file: &mut File,
    prior: Latest,
    kind: super::DeveloperTraceFrameKind,
    payload: &[u8],
) -> Result<(), DeveloperLoopError> {
    if match u64::try_from(payload.len()) {
        Ok(size) => size > MAX_RETRY_RECORD_BYTES,
        Err(_) => true,
    } {
        return Err(failure("retry record exceeds its schema bound"));
    }
    apply_frame(prior, kind, payload)?;
    let location = super::append_locked(file, kind.tag(), payload)
        .map_err(|_| failure("append retry trace boundary"))?;
    file.sync_data().map_err(|_| failure("sync retry trace boundary"))?;
    retain_frontier(
        path,
        FrameAnchor {
            offset: location.offset,
            end: location.end,
            frame_sha256: frame_digest(kind.tag(), payload),
        },
    );
    Ok(())
}

fn scan(path: &Path) -> Result<Latest, DeveloperLoopError> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Latest::None),
        Err(_) => return Err(failure("open retry trace")),
    };
    file.lock().map_err(|_| failure("lock retry trace"))?;
    Ok(scan_file(path, &mut file)?.latest)
}

fn scan_file(path: &Path, file: &mut File) -> Result<Scan, DeveloperLoopError> {
    let trace_bytes = file.metadata().map_err(|_| failure("inspect retry trace"))?.len();
    let recovered = load_frontier(path, file, trace_bytes)?;
    let (mut latest, mut anchor, start) = recovered.map_or_else(
        || (Latest::None, None, 0),
        |record| (record.latest, Some(record.anchor), record.anchor.end),
    );
    let published_anchor = anchor;
    file.seek(std::io::SeekFrom::Start(start))
        .map_err(|_| failure("locate retry trace frontier"))?;
    loop {
        let offset = file
            .stream_position()
            .map_err(|_| failure("locate retry trace frame"))?;
        let mut tag = [0; 1];
        match file.read(&mut tag) {
            Ok(0) => break,
            Ok(1) => {}
            Ok(_) => unreachable!("one-byte read returned more than one byte"),
            Err(_) => return Err(failure("read retry trace tag")),
        }
        let mut size = [0; 8];
        file.read_exact(&mut size)
            .map_err(|_| failure("retry trace has a torn frame header"))?;
        let size = u64::from_le_bytes(size);
        let kind = super::DeveloperTraceFrameKind::from_tag(tag[0])
            .ok_or_else(|| failure("retry trace has an unknown frame kind"))?;
        if matches!(
            kind,
            super::DeveloperTraceFrameKind::RetryScheduled
                | super::DeveloperTraceFrameKind::RetryAttempt
                | super::DeveloperTraceFrameKind::RetryDisposition
        ) {
            if size > MAX_RETRY_RECORD_BYTES {
                return Err(failure("retry record exceeds its schema bound"));
            }
            let mut bytes =
                vec![0; usize::try_from(size).map_err(|_| failure("retry frame size overflow"))?];
            file.read_exact(&mut bytes)
                .map_err(|_| failure("retry trace has a torn frame payload"))?;
            latest = apply_frame(latest, kind, &bytes)?;
            if frontier_state(kind, &bytes)?.is_some() {
                let end = offset
                    .checked_add(9)
                    .and_then(|value| value.checked_add(size))
                    .ok_or_else(|| failure("retry frame position overflow"))?;
                anchor = Some(FrameAnchor {
                    offset,
                    end,
                    frame_sha256: frame_digest(tag[0], &bytes),
                });
            }
        } else {
            let payload = file
                .stream_position()
                .map_err(|_| failure("locate unrelated trace frame"))?;
            let next = payload
                .checked_add(size)
                .ok_or_else(|| failure("unrelated trace frame position overflow"))?;
            if next > trace_bytes {
                return Err(failure("retry trace has a torn frame payload"));
            }
            file.seek(std::io::SeekFrom::Start(next))
                .map_err(|_| failure("skip unrelated trace frame"))?;
        }
    }
    if anchor != published_anchor
        && let Some(anchor) = anchor
    {
        retain_frontier(path, anchor);
    }
    Ok(Scan { latest })
}

fn load_frontier(
    path: &Path,
    trace: &mut File,
    trace_bytes: u64,
) -> Result<Option<RecoveredFrontier>, DeveloperLoopError> {
    let frontier_path = frontier_path(path);
    let frontier = match File::open(&frontier_path) {
        Ok(frontier) => frontier,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            report_frontier_miss(path, format_args!("open failed: {error}"));
            return Ok(None);
        }
    };
    let frontier_bytes = match frontier.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            report_frontier_miss(path, format_args!("metadata read failed: {error}"));
            return Ok(None);
        }
    };
    if frontier_bytes > MAX_FRONTIER_BYTES {
        report_frontier_miss(path, "encoded frontier exceeds its schema bound");
        return Ok(None);
    }
    let mut bytes = Vec::new();
    if let Err(error) = frontier
        .take(MAX_FRONTIER_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
    {
        report_frontier_miss(path, format_args!("read failed: {error}"));
        return Ok(None);
    }
    if !u64::try_from(bytes.len()).is_ok_and(|size| size <= MAX_FRONTIER_BYTES) {
        report_frontier_miss(path, "encoded frontier exceeds its schema bound");
        return Ok(None);
    }
    let record: FrontierRecord = match serde_json::from_slice(&bytes) {
        Ok(record) => record,
        Err(error) => {
            report_frontier_miss(path, format_args!("decode failed: {error}"));
            return Ok(None);
        }
    };
    if record.schema_version != FRONTIER_SCHEMA_VERSION
        || record.anchor_end > trace_bytes
        || record.anchor_offset.checked_add(9).is_none_or(|header| header > record.anchor_end)
    {
        report_frontier_miss(path, "schema or anchor validation failed");
        return Ok(None);
    }
    trace
        .seek(std::io::SeekFrom::Start(record.anchor_offset))
        .map_err(|_| failure("locate retry frontier anchor in authoritative trace"))?;
    let mut tag = [0; 1];
    let mut size = [0; 8];
    trace
        .read_exact(&mut tag)
        .and_then(|()| trace.read_exact(&mut size))
        .map_err(|_| failure("read retry frontier anchor from authoritative trace"))?;
    let size = u64::from_le_bytes(size);
    if size > MAX_RETRY_RECORD_BYTES
        || record
            .anchor_offset
            .checked_add(9)
            .and_then(|value| value.checked_add(size))
            != Some(record.anchor_end)
    {
        report_frontier_miss(path, "anchor frame shape does not match authoritative trace");
        return Ok(None);
    }
    let mut payload =
        vec![0; usize::try_from(size).map_err(|_| failure("frontier size overflow"))?];
    trace
        .read_exact(&mut payload)
        .map_err(|_| failure("read retry frontier payload from authoritative trace"))?;
    if frame_digest(tag[0], &payload) != record.frame_sha256 {
        report_frontier_miss(path, "anchor digest does not match authoritative trace");
        return Ok(None);
    }
    let Some(kind) = super::DeveloperTraceFrameKind::from_tag(tag[0]) else {
        report_frontier_miss(path, "anchor does not identify a known trace frame");
        return Ok(None);
    };
    let evidence = match frontier_state(kind, &payload) {
        Ok(Some(evidence)) => evidence,
        Ok(None) => {
            report_frontier_miss(path, "anchor does not identify retry evidence");
            return Ok(None);
        }
        Err(error) => {
            report_frontier_miss(path, format_args!("anchor retry evidence is invalid: {error}"));
            return Ok(None);
        }
    };
    Ok(Some(RecoveredFrontier {
        latest: evidence,
        anchor: FrameAnchor {
            offset: record.anchor_offset,
            end: record.anchor_end,
            frame_sha256: record.frame_sha256,
        },
    }))
}

fn retain_frontier(path: &Path, anchor: FrameAnchor) {
    if let Err(error) = publish_frontier(path, anchor) {
        crate::diagnostic::report(&format!(
            "peritus retry trace: retained authoritative retry evidence, but could not publish derived frontier {}: {error}",
            frontier_path(path).display()
        ));
    }
}

fn publish_frontier(path: &Path, anchor: FrameAnchor) -> Result<(), String> {
    let bytes = serde_json::to_vec(&FrontierRecord {
        schema_version: FRONTIER_SCHEMA_VERSION,
        anchor_offset: anchor.offset,
        anchor_end: anchor.end,
        frame_sha256: anchor.frame_sha256,
    })
    .map_err(|error| format!("encode failed: {error}"))?;
    if !u64::try_from(bytes.len()).is_ok_and(|size| size <= MAX_FRONTIER_BYTES) {
        return Err("encoded frontier exceeds its schema bound".to_owned());
    }
    let target = frontier_path(path);
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create staging file failed: {error}"))?;
    temporary
        .write_all(&bytes)
        .map_err(|error| format!("write staging file failed: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("sync staging file failed: {error}"))?;
    temporary
        .persist(&target)
        .map_err(|error| format!("replace frontier failed: {}", error.error))?;
    sync_parent(parent).map_err(|error| format!("sync frontier directory failed: {error}"))
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> std::io::Result<()> {
    File::open(parent).and_then(|directory| directory.sync_all())
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

fn report_frontier_miss(path: &Path, detail: impl std::fmt::Display) {
    crate::diagnostic::report(&format!(
        "peritus retry trace: ignored derived frontier {} ({detail}); scanning authoritative trace",
        frontier_path(path).display()
    ));
}

fn frontier_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".retry-frontier");
    PathBuf::from(value)
}

fn frame_digest(tag: u8, payload: &[u8]) -> [u8; 32] {
    let mut frame = Vec::with_capacity(payload.len().saturating_add(9));
    frame.push(tag);
    frame.extend_from_slice(&u64::try_from(payload.len()).unwrap_or(u64::MAX).to_le_bytes());
    frame.extend_from_slice(payload);
    peritus_codec::sha256(&frame).into_bytes()
}

const fn is_retry_kind(kind: super::DeveloperTraceFrameKind) -> bool {
    matches!(
        kind,
        super::DeveloperTraceFrameKind::RetryScheduled
            | super::DeveloperTraceFrameKind::RetryAttempt
            | super::DeveloperTraceFrameKind::RetryDisposition
    )
}

fn frontier_state(
    kind: super::DeveloperTraceFrameKind,
    bytes: &[u8],
) -> Result<Option<Latest>, DeveloperLoopError> {
    match kind {
        super::DeveloperTraceFrameKind::RetryScheduled => {
            let record: ScheduleRecord = match serde_json::from_slice(bytes) {
                Ok(record) => record,
                Err(_) => {
                    let legacy: LegacyScheduleRecord = serde_json::from_slice(bytes)
                        .map_err(|_| failure("invalid retry schedule"))?;
                    validate_legacy_schedule(&legacy)?;
                    return Ok(None);
                }
            };
            validate_schedule(&record)?;
            Ok(Some(Latest::Scheduled(record)))
        }
        super::DeveloperTraceFrameKind::RetryAttempt
        | super::DeveloperTraceFrameKind::RetryDisposition => {
            let record: BoundaryRecord =
                serde_json::from_slice(bytes).map_err(|_| failure("invalid retry boundary"))?;
            validate_boundary(&record)?;
            let state = match (kind, record.disposition.as_str()) {
                (super::DeveloperTraceFrameKind::RetryAttempt, "admitted") => {
                    Latest::Admitted(record)
                }
                (super::DeveloperTraceFrameKind::RetryDisposition, "settled") => {
                    Latest::Settled(record)
                }
                (
                    super::DeveloperTraceFrameKind::RetryDisposition,
                    "reconciliation_required",
                ) => Latest::ReconciliationRequired(record),
                (super::DeveloperTraceFrameKind::RetryDisposition, "superseded") => {
                    Latest::Superseded(record)
                }
                _ => return Err(failure("invalid retry frontier boundary")),
            };
            Ok(Some(state))
        }
        _ => Ok(None),
    }
}

fn apply_frame(
    prior: Latest,
    kind: super::DeveloperTraceFrameKind,
    bytes: &[u8],
) -> Result<Latest, DeveloperLoopError> {
    match kind {
        super::DeveloperTraceFrameKind::RetryScheduled => {
            let record: ScheduleRecord = match serde_json::from_slice(bytes) {
                Ok(record) => record,
                // Legacy retry frames remain readable as history but lack the identity and
                // certainty needed to authorize a restart dispatch.
                Err(_) => {
                    let legacy: LegacyScheduleRecord = serde_json::from_slice(bytes)
                        .map_err(|_| failure("invalid retry schedule"))?;
                    validate_legacy_schedule(&legacy)?;
                    return Ok(prior);
                }
            };
            validate_schedule(&record)?;
            match prior {
                Latest::Admitted(admitted) if same_attempt(&record, &admitted) => {}
                _ => return Err(failure("retry schedule does not settle its admitted attempt")),
            }
            Ok(Latest::Scheduled(record))
        }
        super::DeveloperTraceFrameKind::RetryAttempt
        | super::DeveloperTraceFrameKind::RetryDisposition => {
            let record: BoundaryRecord =
                serde_json::from_slice(bytes).map_err(|_| failure("invalid retry boundary"))?;
            validate_boundary(&record)?;
            match record.disposition.as_str() {
                "admitted" if kind == super::DeveloperTraceFrameKind::RetryAttempt => {
                    let expected = match prior {
                        Latest::None => 1,
                        Latest::Settled(_) | Latest::Superseded(_) => 1,
                        Latest::Scheduled(scheduled) => {
                            if !same_lineage(&scheduled, &record) {
                                return Err(failure(
                                    "retry admission changed its immutable request binding",
                                ));
                            }
                            scheduled
                                .attempt
                                .checked_add(1)
                                .ok_or_else(|| failure("retry attempt identity overflow"))?
                        }
                        Latest::Admitted(_) | Latest::ReconciliationRequired(_) => {
                            return Err(failure(
                                "retry admission follows an unsettled boundary",
                            ));
                        }
                    };
                    if record.attempt != expected {
                        return Err(failure("retry admission sequence is not contiguous"));
                    }
                    Ok(Latest::Admitted(record))
                }
                "settled" if kind == super::DeveloperTraceFrameKind::RetryDisposition => {
                    match prior {
                        Latest::Admitted(admitted) if same_boundary(&admitted, &record) => {
                            Ok(Latest::Settled(record))
                        }
                        _ => Err(failure("retry settlement has no matching admission")),
                    }
                }
                "reconciliation_required"
                    if kind == super::DeveloperTraceFrameKind::RetryDisposition =>
                {
                    match prior {
                        Latest::Admitted(admitted) if same_boundary(&admitted, &record) => {
                            Ok(Latest::ReconciliationRequired(record))
                        }
                        _ => Err(failure("retry reconciliation has no matching admission")),
                    }
                }
                "superseded" if kind == super::DeveloperTraceFrameKind::RetryDisposition => {
                    match prior {
                        Latest::Scheduled(scheduled) if same_attempt(&scheduled, &record) => {
                            Ok(Latest::Superseded(record))
                        }
                        _ => Err(failure(
                            "retry supersession has no matching definitely-unaccepted schedule",
                        )),
                    }
                }
                _ => Err(failure("invalid retry boundary disposition")),
            }
        }
        _ => Ok(prior),
    }
}

fn schedule_matches(
    record: &ScheduleRecord,
    request_prefix: &str,
    turn: u16,
    provider_profile_id: ProviderProfileId,
    expected_native_session: Option<Sha256Digest>,
    expected_request_fingerprint: Sha256Digest,
) -> Result<bool, DeveloperLoopError> {
    validate_schedule(record)?;
    if record.turn != turn
        || record.request_id_sha256
            != expected_digest(request_prefix, turn, record.attempt).into_bytes()
    {
        return Ok(false);
    }
    if record.provider_profile != provider_profile_id.into_bytes()
        || record.native_session_sha256 != expected_native_session.map(Sha256Digest::into_bytes)
        || record.request_fingerprint_sha256 != expected_request_fingerprint.into_bytes()
    {
        return Err(failure("retry schedule immutable request binding changed"));
    }
    Ok(true)
}

fn validate_schedule(record: &ScheduleRecord) -> Result<(), DeveloperLoopError> {
    if !(MIN_SCHEMA_VERSION..=SCHEMA_VERSION).contains(&record.schema_version)
        || (record.schema_version == MIN_SCHEMA_VERSION
            && record.provider_selection_sha256.is_some())
        || record.turn == 0
        || record.attempt == 0
        || record.certainty != "definitely_not_accepted"
        || record.max_attempts.is_some()
        || record.delay_millis == 0
        || record.next_eligible_unix_millis < record.delay_millis
        || record.retry_after_millis.is_some_and(|delay| delay > record.delay_millis)
        || !matches!(record.reason.as_str(), "retryable_provider_response" | "connection")
    {
        return Err(failure("invalid retry schedule state"));
    }
    Ok(())
}

fn validate_legacy_schedule(record: &LegacyScheduleRecord) -> Result<(), DeveloperLoopError> {
    if record.turn == 0
        || record.attempt == 0
        || record.max_attempts == 0
        || record.attempt >= record.max_attempts
        || record.delay_millis == 0
        || record.retry_after_millis.is_some_and(|delay| delay > record.delay_millis)
        || !matches!(
            record.reason.as_str(),
            "empty_response"
                | "retryable_provider_response"
                | "connection"
                | "transport"
                | "malformed_stream"
        )
    {
        return Err(failure("invalid legacy retry schedule state"));
    }
    Ok(())
}

fn boundary_matches(
    record: &BoundaryRecord,
    request_prefix: &str,
    turn: u16,
    provider_profile_id: ProviderProfileId,
    expected_native_session: Option<Sha256Digest>,
    expected_request_fingerprint: Sha256Digest,
) -> Result<bool, DeveloperLoopError> {
    validate_boundary(record)?;
    if record.turn != turn
        || record.request_id_sha256
            != expected_digest(request_prefix, turn, record.attempt).into_bytes()
    {
        return Ok(false);
    }
    if record.provider_profile != provider_profile_id.into_bytes()
        || record.native_session_sha256 != expected_native_session.map(Sha256Digest::into_bytes)
        || record.request_fingerprint_sha256 != expected_request_fingerprint.into_bytes()
    {
        return Err(failure("retry boundary immutable request binding changed"));
    }
    Ok(true)
}

fn validate_boundary(record: &BoundaryRecord) -> Result<(), DeveloperLoopError> {
    if !(MIN_SCHEMA_VERSION..=SCHEMA_VERSION).contains(&record.schema_version)
        || (record.schema_version == MIN_SCHEMA_VERSION
            && record.superseded_by_provider_selection_sha256.is_some())
        || record.turn == 0
        || record.attempt == 0
    {
        return Err(failure("invalid retry boundary state"));
    }
    if record.superseded_by_provider_selection_sha256.is_some()
        && record.disposition != "superseded"
    {
        return Err(failure("invalid retry boundary provider-selection provenance"));
    }
    Ok(())
}

fn same_attempt(schedule: &ScheduleRecord, boundary: &BoundaryRecord) -> bool {
    schedule.turn == boundary.turn
        && schedule.attempt == boundary.attempt
        && schedule.request_id_sha256 == boundary.request_id_sha256
        && same_lineage(schedule, boundary)
}

fn same_lineage(schedule: &ScheduleRecord, boundary: &BoundaryRecord) -> bool {
    schedule.turn == boundary.turn
        && schedule.request_fingerprint_sha256 == boundary.request_fingerprint_sha256
        && schedule.provider_profile == boundary.provider_profile
        && schedule.native_session_sha256 == boundary.native_session_sha256
}

fn same_boundary(left: &BoundaryRecord, right: &BoundaryRecord) -> bool {
    left.turn == right.turn
        && left.attempt == right.attempt
        && left.request_id_sha256 == right.request_id_sha256
        && left.request_fingerprint_sha256 == right.request_fingerprint_sha256
        && left.provider_profile == right.provider_profile
        && left.native_session_sha256 == right.native_session_sha256
        && left.superseded_by_provider_selection_sha256
            == right.superseded_by_provider_selection_sha256
}

fn expected_digest(request_prefix: &str, turn: u16, attempt: u64) -> Sha256Digest {
    peritus_codec::sha256(format!("{request_prefix}-{turn}-attempt-{attempt}").as_bytes())
}

fn request_digest(request: &ModelRequest) -> [u8; 32] {
    peritus_codec::sha256(request.request_id().expose_for_wire().as_bytes()).into_bytes()
}

fn native_session_digest(request: &ModelRequest) -> Option<[u8; 32]> {
    request.local_session_directory().map(|directory| {
        peritus_codec::sha256(directory.as_os_str().as_encoded_bytes()).into_bytes()
    })
}

fn failure(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
