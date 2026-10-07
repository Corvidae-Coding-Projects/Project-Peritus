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
    model_requests: u32,
    provider_started: u64,
    activity_frontier: u64,
    elapsed_seconds: u64,
}

#[cfg(not(verus_only))]
pub(super) fn capture(
    service: &ProductRunService,
    run: RunId,
    elapsed_seconds: u64,
) -> Result<Notice, DeveloperLoopError> {
    let records = service.inner.records.try_read().map_err(|error| {
        DeveloperLoopError::Trace(format!("waiting projection is unavailable: {error}"))
    })?;
    let record = records.get(&run).ok_or_else(|| {
        DeveloperLoopError::Trace("waiting projection has no exact run owner".to_owned())
    })?;
    let mut notice = pending(record).ok_or_else(|| {
        DeveloperLoopError::Trace("waiting projection has no pending provider request".to_owned())
    })?;
    notice.elapsed_seconds = elapsed_seconds;
    Ok(notice)
}

pub(in crate::product_run::interaction) fn pending(
    record: &crate::product_run::RunRecord,
) -> Option<Notice> {
    if record.cancelled.load(Ordering::Acquire)
        || record.interaction.persistence_failed.load(Ordering::Acquire)
    {
        return None;
    }
    Some(Notice {
        schema: 1,
        run: *record.request.run_id().as_bytes(),
        start_operation: *record.interaction.workbench.id().as_bytes(),
        model_requests: record.progress.model_requests,
        provider_started: record.progress.provider_started_unix_millis?,
        activity_frontier: record.interaction.next_sequence,
        elapsed_seconds: 0,
    })
}

pub(super) fn retain(
    service: &ProductRunService,
    notice: &Notice,
) -> Result<(), ProductRunServiceError> {
    let path = path(service, &notice.run, notice.model_requests, notice.provider_started, notice.activity_frontier)?;
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
        let path = path(service, &expected.run, expected.model_requests, expected.provider_started, expected.activity_frontier)?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ProductRunServiceError::persistence("read waiting status outbox", error)),
        };
        let notice: Notice = serde_json::from_reader(std::io::BufReader::new(file)).map_err(|error| {
            ProductRunServiceError::persistence("decode waiting status outbox", error)
        })?;
        if notice.schema != 1 || notice.run != expected.run {
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
        && notice.model_requests == expected.model_requests
        && notice.provider_started == expected.provider_started
        && notice.activity_frontier == expected.activity_frontier
}

fn path(
    service: &ProductRunService,
    run: &[u8; 16],
    model_requests: u32,
    provider_started: u64,
    activity_frontier: u64,
) -> Result<PathBuf, ProductRunServiceError> {
    use std::fmt::Write as _;
    let mut identity = String::with_capacity(32);
    for byte in run {
        write!(&mut identity, "{byte:02x}").map_err(|_| ProductRunServiceError::Unavailable)?;
    }
    Ok(crate::product_run::persistence::record_directory(&service.inner.directory)?
        .join(".waiting-outbox")
        .join(identity)
        .join(format!("{model_requests}-{provider_started}-{activity_frontier}.json")))
}
