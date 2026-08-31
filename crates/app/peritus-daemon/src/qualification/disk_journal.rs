//! Deterministic authoritative-journal page exhaustion and rollback verification.

use peritus_codec::{CodecLimits, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, EventDraft,
    ExactFrame, HeadExpectation, SqliteJournal, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

const LARGE_FRAME_BYTES: usize = 2 * 1024 * 1024;
const FRAME_FAMILY: u16 = 65_002;

/// Exact facts after `SQLite` rejected journal growth at its durable page ceiling.
pub struct JournalDiskCheckpoint {
    request_sha256: String,
    page_count: u64,
    page_size: u64,
    maximum_bytes: u64,
}

impl JournalDiskCheckpoint {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub(crate) const fn page_count(&self) -> u64 {
        self.page_count
    }
    pub(crate) const fn page_size(&self) -> u64 {
        self.page_size
    }
    pub(crate) const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }
}

/// Fresh-process facts proving the exhausted append left no authoritative rows.
pub struct JournalDiskQualification {
    request_sha256: String,
    page_count: u64,
    page_size: u64,
    maximum_bytes: u64,
    committed_events: u64,
    aggregate_heads: u64,
}

impl JournalDiskQualification {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub(crate) const fn page_count(&self) -> u64 {
        self.page_count
    }
    pub(crate) const fn page_size(&self) -> u64 {
        self.page_size
    }
    pub(crate) const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }
    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub(crate) const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }
}

/// Applies the production `SQLite` page ceiling and requires one real append to fail atomically.
pub fn stage_journal_append_exhaustion(
    config: &DaemonConfig,
) -> Result<JournalDiskCheckpoint, DaemonError> {
    let store = config.store_identity()?;
    let _instance = acquire_instance(config, store)?;
    let mut journal = open_journal(config, store)?;
    require_empty(&mut journal)?;
    let identity = Identity::new(store)?;
    let baseline = journal.storage_pages().map_err(journal_error)?;
    let limited = journal.limit_storage_pages(baseline.page_count()).map_err(journal_error)?;
    let plan = build_plan(&journal, &identity)?;
    let error = match journal.append(plan) {
        Ok(_) => return Err(storage_error("journal append exceeded its page ceiling")),
        Err(error) => error,
    };
    if !error.is_storage_exhausted() {
        return Err(storage_error(
            "journal append failed for a reason other than storage exhaustion",
        ));
    }
    require_absent(&mut journal, &identity)?;
    Ok(JournalDiskCheckpoint {
        request_sha256: hex(identity.request),
        page_count: limited.page_count(),
        page_size: limited.page_size(),
        maximum_bytes: limited.maximum_bytes(),
    })
}

/// Reopens the bounded journal and verifies the rejected append is definitely absent.
pub fn recover_journal_append_exhaustion(
    config: &DaemonConfig,
) -> Result<JournalDiskQualification, DaemonError> {
    let store = config.store_identity()?;
    let _instance = acquire_instance(config, store)?;
    let mut journal = open_journal(config, store)?;
    let identity = Identity::new(store)?;
    require_absent(&mut journal, &identity)?;
    let pages = journal.storage_pages().map_err(journal_error)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    Ok(JournalDiskQualification {
        request_sha256: hex(identity.request),
        page_count: pages.page_count(),
        page_size: pages.page_size(),
        maximum_bytes: pages.maximum_bytes(),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
    })
}

struct Identity {
    aggregate: AggregateKey,
    command: CommandId,
    event: EventId,
    request: Sha256Digest,
}

impl Identity {
    fn new(store: StoreId) -> Result<Self, DaemonError> {
        Ok(Self {
            aggregate: AggregateKey::new(
                AggregateKind::Application,
                AggregateId::new(id(b"peritus/h1/disk-journal/aggregate/v1\0", store))
                    .map_err(journal_error)?,
            ),
            command: CommandId::new(id(b"peritus/h1/disk-journal/command/v1\0", store))
                .map_err(|_| storage_error("journal quota command identity is invalid"))?,
            event: EventId::new(id(b"peritus/h1/disk-journal/event/v1\0", store))
                .map_err(|_| storage_error("journal quota event identity is invalid"))?,
            request: digest(b"peritus/h1/disk-journal/request/v1\0", store),
        })
    }
}

fn build_plan(
    journal: &SqliteJournal,
    identity: &Identity,
) -> Result<peritus_journal::AppendPlan, DaemonError> {
    let payload = vec![0xA7; LARGE_FRAME_BYTES];
    let frame = encode_frame(FRAME_FAMILY, 1, &payload, CodecLimits::PRODUCTION)
        .map_err(|_| storage_error("encode journal quota frame"))?;
    let event = EventDraft::new(
        identity.aggregate,
        EventSequence::first(),
        identity.event,
        None,
        ExactFrame::new(frame).map_err(journal_error)?,
        digest(b"peritus/h1/disk-journal/revision/v1\0", journal.store_id()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    AppendRequest::new(
        journal.store_id(),
        identity.command,
        identity.request,
        vec![HeadExpectation::Absent(identity.aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .map_err(journal_error)
}

fn require_empty(journal: &mut SqliteJournal) -> Result<(), DaemonError> {
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() == 0 && report.aggregate_count() == 0 {
        Ok(())
    } else {
        Err(storage_error("journal quota qualification requires an empty journal"))
    }
}

fn require_absent(journal: &mut SqliteJournal, identity: &Identity) -> Result<(), DaemonError> {
    let resolution =
        journal.resolve_command(identity.command, identity.request).map_err(journal_error)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    if matches!(resolution, CommandResolution::DefinitelyAbsent)
        && report.event_count() == 0
        && report.aggregate_count() == 0
        && journal.head(identity.aggregate).map_err(journal_error)?.is_none()
    {
        Ok(())
    } else {
        Err(storage_error("exhausted journal append left partial authoritative state"))
    }
}

fn digest(domain: &[u8], store: StoreId) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(domain.len() + 16);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(store.as_bytes());
    sha256(&bytes)
}

fn id(domain: &[u8], store: StoreId) -> [u8; 16] {
    let digest = digest(domain, store);
    let mut value = [0; 16];
    value.copy_from_slice(&digest.as_bytes()[..16]);
    value
}

fn hex(value: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in value.as_bytes() {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn storage_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify journal append storage exhaustion",
        detail,
    )
}
