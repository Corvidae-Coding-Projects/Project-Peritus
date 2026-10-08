//! A coalesced durable status outbox; it cannot cancel or admit provider work.

use super::{ProductRunService, ProductRunServiceError};
#[cfg(not(verus_only))]
use peritus_agent::DeveloperLoopError;
use peritus_app_protocol::{ProductActivity, ProductActivityKind};
use peritus_types::RunId;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write as _, path::PathBuf, sync::atomic::Ordering};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::product_run::interaction) struct Notice {
    schema: u32,
    run: [u8; 16],
    start_operation: [u8; 16],
    attempt_sequence: u64,
    model_requests: u32,
    provider_started: u64,
    activity_frontier: u64,
    role: u8,
    turn: u16,
    provider_attempt: u64,
    request_id_digest: [u8; 32],
    request_fingerprint: [u8; 32],
    provider_profile_id: [u8; 16],
    provider_profile_revision: u64,
    provider_name_digest: [u8; 32],
    native_session_digest: Option<[u8; 32]>,
    provider_selection_digest: Option<[u8; 32]>,
    elapsed_seconds: u64,
}

#[cfg(not(verus_only))]
pub(super) fn capture(
    service: &ProductRunService,
    run: RunId,
    expected_attempt: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    elapsed_seconds: u64,
) -> Result<Notice, DeveloperLoopError> {
    let records = service.inner.records.try_read().map_err(|error| {
        DeveloperLoopError::Trace(format!("waiting projection is unavailable: {error}"))
    })?;
    let record = records.get(&run).ok_or_else(|| {
        DeveloperLoopError::Trace("waiting projection has no exact run owner".to_owned())
    })?;
    if !std::sync::Arc::ptr_eq(expected_attempt, &record.cancelled) {
        return Err(DeveloperLoopError::Trace(
            "waiting projection belongs to a superseded execution attempt".to_owned(),
        ));
    }
    let mut notice = pending(record).ok_or_else(|| {
        DeveloperLoopError::Trace("waiting projection has no pending provider request".to_owned())
    })?;
    notice.elapsed_seconds = elapsed_seconds;
    Ok(notice)
}

pub(in crate::product_run::interaction) fn pending(
    record: &crate::product_run::RunRecord,
) -> Option<Notice> {
    if record.cancelled.load(Ordering::Acquire) {
        return None;
    }
    let request = record.progress.provider_request?;
    Some(Notice {
        schema: 2,
        run: *record.request.run_id().as_bytes(),
        start_operation: *record.interaction.workbench.id().as_bytes(),
        attempt_sequence: record.attempt_sequence,
        model_requests: record.progress.model_requests,
        provider_started: record.progress.provider_started_unix_millis?,
        activity_frontier: record.interaction.next_sequence,
        role: request.role,
        turn: request.turn,
        provider_attempt: request.attempt,
        request_id_digest: request.request_id_digest,
        request_fingerprint: request.request_fingerprint,
        provider_profile_id: request.provider_profile_id,
        provider_profile_revision: request.provider_profile_revision,
        provider_name_digest: request.provider_name_digest,
        native_session_digest: request.native_session_digest,
        provider_selection_digest: request.provider_selection_digest,
        elapsed_seconds: 0,
    })
}

pub(super) fn retain(
    service: &ProductRunService,
    notice: &Notice,
) -> Result<(), ProductRunServiceError> {
    let path = path(service, notice)?;
    let parent = path.parent().ok_or(ProductRunServiceError::Unavailable)?;
    fs::create_dir_all(parent).map_err(|error| {
        ProductRunServiceError::persistence("create waiting status outbox", error)
    })?;
    let bytes = serde_json::to_vec(notice).map_err(|error| {
        ProductRunServiceError::persistence("encode waiting status outbox", error)
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        ProductRunServiceError::persistence("stage waiting status outbox", error)
    })?;
    temporary.write_all(&bytes).map_err(|error| {
        ProductRunServiceError::persistence("write waiting status outbox", error)
    })?;
    temporary.as_file().sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync waiting status outbox", error)
    })?;
    let records = service.inner.records.try_read().map_err(|_| ProductRunServiceError::Unavailable)?;
    let run = RunId::new(notice.run).map_err(|_| ProductRunServiceError::InvalidMessage)?;
    let Some(record) = records.get(&run) else { return Ok(()) };
    if !current(notice, record) {
        return Ok(());
    }
    // Each provider frontier owns a separate slot. A superseded worker cannot overwrite the
    // next attempt's notice, and filesystem publication never holds the authoritative run lock.
    drop(records);
    temporary.persist(&path).map_err(|error| {
        ProductRunServiceError::persistence("publish waiting status outbox", error)
    })?;
    #[cfg(unix)]
    fs::File::open(parent).and_then(|file| file.sync_all()).map_err(|error| {
        ProductRunServiceError::persistence("sync waiting status outbox directory", error)
    })?;
    Ok(())
}

pub(in crate::product_run::interaction) fn project(
    service: &ProductRunService,
    expected: &Notice,
    activities: &mut Vec<ProductActivity>,
) {
    let result = (|| -> Result<(), ProductRunServiceError> {
        let path = path(service, expected)?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ProductRunServiceError::persistence("read waiting status outbox", error)),
        };
        let notice: Notice = serde_json::from_reader(std::io::BufReader::new(file)).map_err(|error| {
            ProductRunServiceError::persistence("decode waiting status outbox", error)
        })?;
        if notice.schema != 2 || notice.run != expected.run {
            return Err(ProductRunServiceError::internal("read waiting status outbox", "the notice belongs to another run or schema"));
        }
        // Later activity, provider settlement, explicit cancellation, or a new attempt supersedes
        // this status. The retained sidecar never becomes new input or a recovered effect receipt.
        if !same_frontier(&notice, expected) {
            return Ok(());
        }
        let activity = ProductActivity::new(
            notice.activity_frontier,
            ProductActivityKind::Status,
            format!("Still waiting for public provider output ({}s so far).", notice.elapsed_seconds),
            "Durable host status; no new provider result is available.".to_owned(),
        ).map_err(|error| ProductRunServiceError::invalid_data("project waiting status", error))?;
        activities.push(activity);
        Ok(())
    })();
    if let Err(error) = result {
        crate::diagnostic::report(&format!(
            "peritusd: retained waiting status could not be projected: {error}; execution ownership is unchanged",
        ));
    }
}

fn current(notice: &Notice, record: &crate::product_run::RunRecord) -> bool {
    pending(record).is_some_and(|expected| same_frontier(notice, &expected))
}

fn same_frontier(notice: &Notice, expected: &Notice) -> bool {
    notice.run == expected.run
        && notice.start_operation == expected.start_operation
        && notice.attempt_sequence == expected.attempt_sequence
        && notice.model_requests == expected.model_requests
        && notice.provider_started == expected.provider_started
        && notice.activity_frontier == expected.activity_frontier
        && notice.role == expected.role
        && notice.turn == expected.turn
        && notice.provider_attempt == expected.provider_attempt
        && notice.request_id_digest == expected.request_id_digest
        && notice.request_fingerprint == expected.request_fingerprint
        && notice.provider_profile_id == expected.provider_profile_id
        && notice.provider_profile_revision == expected.provider_profile_revision
        && notice.provider_name_digest == expected.provider_name_digest
        && notice.native_session_digest == expected.native_session_digest
        && notice.provider_selection_digest == expected.provider_selection_digest
}

fn path(service: &ProductRunService, notice: &Notice) -> Result<PathBuf, ProductRunServiceError> {
    let identity = hex(&notice.run)?;
    let request = hex(&notice.request_id_digest)?;
    Ok(crate::product_run::persistence::record_directory(&service.inner.directory)?
        .join(".waiting-outbox")
        .join(identity)
        .join(format!(
            "{}-{}-{}-{}-{}-{}.json",
            notice.attempt_sequence,
            notice.model_requests,
            notice.provider_started,
            notice.activity_frontier,
            notice.provider_attempt,
            request,
        )))
}

fn hex(bytes: &[u8]) -> Result<String, ProductRunServiceError> {
    use std::fmt::Write as _;
    let mut identity = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut identity, "{byte:02x}")
            .map_err(|_| ProductRunServiceError::Unavailable)?;
    }
    Ok(identity)
}
