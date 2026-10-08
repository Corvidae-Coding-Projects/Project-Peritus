//! Append-only reconnect receipts and rebuildable product projections.

use peritus_process::ExecutionPlan;
use peritus_tool_protocol::{PreparedToolCall, SchemaDigest};
use peritus_types::{ActionId, EventId, ProcessId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use super::{CommandExecutionMode, result};

const DATABASE: &str = "command-projections.sqlite3";
const LEGACY_FILE: &str = "command-handles-v1.json";
const RECEIPT_PAGE_SIZE: usize = 256;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS command_reconnect_receipts_v1 (
    action_id BLOB PRIMARY KEY CHECK(length(action_id) = 16),
    process_id BLOB NOT NULL CHECK(length(process_id) = 16),
    process_action_digest BLOB NOT NULL CHECK(length(process_action_digest) = 32),
    replay_identity BLOB NOT NULL CHECK(length(replay_identity) = 32),
    prepared_digest BLOB NOT NULL CHECK(length(prepared_digest) = 32),
    descriptor_digest BLOB NOT NULL CHECK(length(descriptor_digest) = 32),
    plan_digest BLOB NOT NULL CHECK(length(plan_digest) = 32),
    plan BLOB NOT NULL,
    creating_event BLOB NOT NULL CHECK(length(creating_event) = 16),
    idempotency_key TEXT NOT NULL,
    interactive INTEGER NOT NULL CHECK(interactive IN (0, 1)),
    mode INTEGER NOT NULL CHECK(mode IN (1, 2)),
    resource_evidence BLOB,
    protected_paths BLOB NOT NULL,
    receipt_digest BLOB NOT NULL CHECK(length(receipt_digest) = 32)
) STRICT, WITHOUT ROWID;
CREATE TRIGGER IF NOT EXISTS command_reconnect_receipts_v1_no_update
BEFORE UPDATE ON command_reconnect_receipts_v1
BEGIN
    SELECT RAISE(ABORT, 'command reconnect receipts are immutable');
END;
CREATE TRIGGER IF NOT EXISTS command_reconnect_receipts_v1_no_delete
BEFORE DELETE ON command_reconnect_receipts_v1
BEGIN
    SELECT RAISE(ABORT, 'command reconnect receipts are immutable');
END;
CREATE TABLE IF NOT EXISTS command_projection_events_v2 (
    action_id BLOB NOT NULL CHECK(length(action_id) = 16),
    sequence BLOB NOT NULL CHECK(length(sequence) = 8),
    value BLOB NOT NULL,
    event_digest BLOB NOT NULL CHECK(length(event_digest) = 32),
    PRIMARY KEY(action_id, sequence)
) STRICT, WITHOUT ROWID;
CREATE TRIGGER IF NOT EXISTS command_projection_events_v2_no_update
BEFORE UPDATE ON command_projection_events_v2
BEGIN
    SELECT RAISE(ABORT, 'command projection events are immutable');
END;
CREATE TRIGGER IF NOT EXISTS command_projection_events_v2_no_delete
BEFORE DELETE ON command_projection_events_v2
BEGIN
    SELECT RAISE(ABORT, 'command projection events are immutable');
END;
";

/// Immutable command-to-process material needed to prove and reconstruct one accepted owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReconnectReceipt {
    pub(super) action_id: ActionId,
    pub(super) process_id: ProcessId,
    pub(super) process_action_digest: Sha256Digest,
    pub(super) replay_identity: Sha256Digest,
    pub(super) prepared_digest: Sha256Digest,
    pub(super) descriptor_digest: SchemaDigest,
    pub(super) plan_digest: Sha256Digest,
    pub(super) plan: Vec<u8>,
    pub(super) creating_event: EventId,
    pub(super) idempotency_key: String,
    pub(super) interactive: bool,
    pub(super) mode: CommandExecutionMode,
    pub(super) resource_evidence: Option<Value>,
    pub(super) protected_paths: Vec<PathBuf>,
}

impl ReconnectReceipt {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        prepared: &PreparedToolCall,
        plan: &ExecutionPlan,
        process_action_digest: Sha256Digest,
        creating_event: EventId,
        idempotency_key: String,
        interactive: bool,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
        protected_paths: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let identity = plan.identity();
        if prepared.call().action_id() != identity.action_id()
            || prepared.prepared_digest()
                != plan
                    .caller_binding()
                    .ok_or_else(|| "command reconnect plan has no caller binding".to_owned())?
                    .prepared_digest()
        {
            return Err("command reconnect receipt differs from its caller-bound plan".to_owned());
        }
        Ok(Self {
            action_id: identity.action_id(),
            process_id: identity.process_id(),
            process_action_digest,
            replay_identity: prepared.replay_identity().digest(),
            prepared_digest: prepared.prepared_digest(),
            descriptor_digest: prepared.descriptor_digest(),
            plan_digest: plan.digest(),
            plan: plan.canonical_bytes().to_vec(),
            creating_event,
            idempotency_key,
            interactive,
            mode,
            resource_evidence,
            protected_paths,
        })
    }
}

/// Path-backed append-only reconnect store.
pub(super) struct Store {
    path: PathBuf,
}

impl Store {
    pub(super) fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root)
            .map_err(|error| format!("create command projection root: {error}"))?;
        let path = root.join(DATABASE);
        let connection = connect(&path)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|error| format!("initialize command reconnect store: {error}"))?;
        let store = Self { path };
        store.migrate_legacy(root)?;
        Ok(store)
    }

    pub(super) fn record_receipt(&self, receipt: &ReconnectReceipt) -> Result<(), String> {
        let resource_evidence = receipt
            .resource_evidence
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|error| format!("encode command reconnect resources: {error}"))?;
        let protected_paths = encode_paths(&receipt.protected_paths)?;
        let digest = reconnect_digest(receipt, resource_evidence.as_deref(), &protected_paths);
        let connection = connect(&self.path)?;
        match connection.execute(
            "INSERT INTO command_reconnect_receipts_v1(
                action_id, process_id, process_action_digest, replay_identity, prepared_digest,
                descriptor_digest, plan_digest, plan, creating_event, idempotency_key,
                interactive, mode, resource_evidence, protected_paths, receipt_digest
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                receipt.action_id.as_bytes().as_slice(),
                receipt.process_id.as_bytes().as_slice(),
                receipt.process_action_digest.as_bytes().as_slice(),
                receipt.replay_identity.as_bytes().as_slice(),
                receipt.prepared_digest.as_bytes().as_slice(),
                receipt.descriptor_digest.as_bytes().as_slice(),
                receipt.plan_digest.as_bytes().as_slice(),
                &receipt.plan,
                receipt.creating_event.as_bytes().as_slice(),
                &receipt.idempotency_key,
                if receipt.interactive { 1_i64 } else { 0_i64 },
                mode_tag(receipt.mode),
                resource_evidence,
                protected_paths,
                digest.as_bytes().as_slice(),
            ],
        ) {
            Ok(_) => Ok(()),
            Err(error) if is_constraint(&error) => {
                let stored = self.receipt(receipt.action_id)?.ok_or_else(|| {
                    "command reconnect receipt conflicted but could not be reread".to_owned()
                })?;
                if stored == *receipt {
                    Ok(())
                } else {
                    Err("command action is already bound to a different reconnect receipt".to_owned())
                }
            }
            Err(error) => Err(format!("record command reconnect receipt: {error}")),
        }
    }

    pub(super) fn receipt(&self, action_id: ActionId) -> Result<Option<ReconnectReceipt>, String> {
        let connection = connect(&self.path)?;
        let row = connection
            .query_row(
                "SELECT process_id, process_action_digest, replay_identity, prepared_digest,
                        descriptor_digest, plan_digest, plan, creating_event, idempotency_key,
                        interactive, mode, resource_evidence, protected_paths, receipt_digest
                 FROM command_reconnect_receipts_v1 WHERE action_id = ?1",
                params![action_id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, Option<Vec<u8>>>(11)?,
                        row.get::<_, Vec<u8>>(12)?,
                        row.get::<_, Vec<u8>>(13)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("read command reconnect receipt: {error}"))?;
        row.map(|row| decode_receipt(action_id, row)).transpose()
    }

    pub(super) fn receipt_for_handle(
        &self,
        handle: &str,
    ) -> Result<Option<ReconnectReceipt>, String> {
        let Some(action_id) = parse_action(handle) else { return Ok(None) };
        self.receipt(action_id)
    }

    /// Reads the next bounded physical page of immutable reconnect receipts.
    ///
    /// The page size is an allocation unit rather than a lifetime history limit. Supplying the
    /// final action identity from one page continues the same ordered scan.
    pub(super) fn receipts_page(
        &self,
        after: Option<ActionId>,
    ) -> Result<Vec<ReconnectReceipt>, String> {
        let connection = connect(&self.path)?;
        let mut statement = connection
            .prepare(
                "SELECT action_id, process_id, process_action_digest, replay_identity,
                        prepared_digest, descriptor_digest, plan_digest, plan, creating_event,
                        idempotency_key, interactive, mode, resource_evidence, protected_paths,
                        receipt_digest
                 FROM command_reconnect_receipts_v1
                 WHERE (?1 IS NULL OR action_id > ?1)
                 ORDER BY action_id LIMIT ?2",
            )
            .map_err(|error| format!("prepare command reconnect receipt page: {error}"))?;
        let after = after.map(|action_id| action_id.as_bytes().to_vec());
        let limit = i64::try_from(RECEIPT_PAGE_SIZE)
            .map_err(|_| "command reconnect receipt page size is not representable".to_owned())?;
        let mut rows = statement
            .query(params![after, limit])
            .map_err(|error| format!("read command reconnect receipt page: {error}"))?;
        let mut receipts = Vec::with_capacity(RECEIPT_PAGE_SIZE);
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("advance command reconnect receipt page: {error}"))?
        {
            let action_id = decode_id(
                row.get::<_, Vec<u8>>(0)
                    .map_err(|error| format!("decode command reconnect action: {error}"))?,
                ActionId::new,
                "action identity",
            )?;
            let fields = (
                row.get::<_, Vec<u8>>(1),
                row.get::<_, Vec<u8>>(2),
                row.get::<_, Vec<u8>>(3),
                row.get::<_, Vec<u8>>(4),
                row.get::<_, Vec<u8>>(5),
                row.get::<_, Vec<u8>>(6),
                row.get::<_, Vec<u8>>(7),
                row.get::<_, Vec<u8>>(8),
                row.get::<_, String>(9),
                row.get::<_, i64>(10),
                row.get::<_, i64>(11),
                row.get::<_, Option<Vec<u8>>>(12),
                row.get::<_, Vec<u8>>(13),
                row.get::<_, Vec<u8>>(14),
            );
            receipts.push(decode_receipt(
                action_id,
                (
                    fields.0.map_err(|error| format!("decode reconnect process: {error}"))?,
                    fields.1.map_err(|error| format!("decode reconnect process action: {error}"))?,
                    fields.2.map_err(|error| format!("decode reconnect replay: {error}"))?,
                    fields.3.map_err(|error| format!("decode reconnect preparation: {error}"))?,
                    fields.4.map_err(|error| format!("decode reconnect descriptor: {error}"))?,
                    fields.5.map_err(|error| format!("decode reconnect plan digest: {error}"))?,
                    fields.6.map_err(|error| format!("decode reconnect plan: {error}"))?,
                    fields.7.map_err(|error| format!("decode reconnect event: {error}"))?,
                    fields.8.map_err(|error| format!("decode reconnect key: {error}"))?,
                    fields.9.map_err(|error| format!("decode reconnect interactive flag: {error}"))?,
                    fields.10.map_err(|error| format!("decode reconnect mode: {error}"))?,
                    fields.11.map_err(|error| format!("decode reconnect resources: {error}"))?,
                    fields.12.map_err(|error| format!("decode reconnect paths: {error}"))?,
                    fields.13.map_err(|error| format!("decode reconnect receipt digest: {error}"))?,
                ),
            )?);
        }
        Ok(receipts)
    }

    pub(super) fn lookup(&self, handle: &str) -> Result<Option<Value>, String> {
        let Some(action_id) = parse_action(handle) else { return Ok(None) };
        self.lookup_action(action_id)
    }

    pub(super) fn record(&self, handle: &str, value: Value) -> Result<(), String> {
        let action_id = parse_action(handle)
            .ok_or_else(|| "command projection handle is not an action identity".to_owned())?;
        self.append_projection(action_id, value, true)
    }

    pub(super) fn repair(&self, handle: &str, value: Value) -> Result<(), String> {
        let action_id = parse_action(handle)
            .ok_or_else(|| "command projection handle is not an action identity".to_owned())?;
        self.append_projection(action_id, value, false)
    }

    pub(super) fn record_deferred(&self, handle: &str, value: Value) -> Result<bool, String> {
        if self.lookup(handle)?.is_some_and(|current| {
            current.get("state").and_then(Value::as_str) == Some("completed")
                && current.get("preview_pending").and_then(Value::as_bool) != Some(true)
        }) {
            return Ok(false);
        }
        self.record(handle, value)?;
        Ok(true)
    }

    fn lookup_action(&self, action_id: ActionId) -> Result<Option<Value>, String> {
        let connection = connect(&self.path)?;
        let mut statement = connection
            .prepare(
                "SELECT sequence, value, event_digest FROM command_projection_events_v2
                 WHERE action_id = ?1 ORDER BY sequence DESC",
            )
            .map_err(|error| format!("prepare command projection lookup: {error}"))?;
        let mut rows = statement
            .query(params![action_id.as_bytes().as_slice()])
            .map_err(|error| format!("read command projection history: {error}"))?;
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("advance command projection history: {error}"))?
        {
            let sequence: Vec<u8> = row
                .get(0)
                .map_err(|error| format!("decode command projection sequence: {error}"))?;
            let value: Vec<u8> = row
                .get(1)
                .map_err(|error| format!("decode command projection value: {error}"))?;
            let digest: Vec<u8> = row
                .get(2)
                .map_err(|error| format!("decode command projection digest: {error}"))?;
            match decode_projection(action_id, &sequence, &value, &digest) {
                Ok(value) => return Ok(Some(value)),
                Err(error) => crate::diagnostic::report(&format!(
                    "peritus command runtime: ignored corrupt derived projection {}: {error}",
                    super::identity::action_hex(action_id),
                )),
            }
        }
        Ok(None)
    }

    fn append_projection(
        &self,
        action_id: ActionId,
        value: Value,
        protect_terminal: bool,
    ) -> Result<(), String> {
        if protect_terminal && matches!(
            value.get("state").and_then(Value::as_str),
            Some("running" | "starting")
        ) && self.lookup_action(action_id)?.is_some_and(|current| {
            matches!(
                current.get("state").and_then(Value::as_str),
                Some("completed" | "indeterminate")
            )
        }) {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("encode command projection: {error}"))?;
        let mut connection = connect(&self.path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("begin command projection append: {error}"))?;
        let latest: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT sequence FROM command_projection_events_v2
                 WHERE action_id = ?1 ORDER BY sequence DESC LIMIT 1",
                params![action_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("read command projection frontier: {error}"))?;
        let sequence = latest
            .map(|bytes| decode_u64(&bytes, "command projection frontier"))
            .transpose()?
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "command projection sequence overflowed".to_owned())?;
        let digest = projection_digest(action_id, sequence, &bytes);
        transaction
            .execute(
                "INSERT INTO command_projection_events_v2(
                    action_id, sequence, value, event_digest
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    action_id.as_bytes().as_slice(),
                    sequence.to_be_bytes().as_slice(),
                    bytes,
                    digest.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| format!("append command projection event: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("commit command projection event: {error}"))
    }

    fn migrate_legacy(&self, root: &Path) -> Result<(), String> {
        let path = root.join(LEGACY_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("read legacy command projections: {error}")),
        };
        let file = match serde_json::from_slice::<LegacyProjectionFile>(&bytes) {
            Ok(file) if file.version == 1 => file,
            Ok(_) | Err(_) => {
                quarantine(&path);
                return Ok(());
            }
        };
        for entry in file.entries {
            let Some(action_id) = parse_action(&entry.handle) else { continue };
            // A partially completed migration or a newer runtime may already have authoritative
            // derived history for this action. Legacy cache bytes must never become a newer event
            // or grow the append-only history again on every open.
            if self.lookup_action(action_id)?.is_some() {
                continue;
            }
            let value = if entry.value.get("state").and_then(Value::as_str) == Some("running") {
                let mode = entry.value.get("execution_mode").cloned();
                let resources = entry.value.get("execution_resources").cloned();
                let mut value = result::indeterminate(
                    &entry.handle,
                    "the legacy reconnect cache recorded an active command without an authoritative owner receipt",
                );
                if let (Some(mode), Some(object)) = (mode, value.as_object_mut()) {
                    object.insert("execution_mode".to_owned(), mode);
                }
                if let (Some(resources), Some(object)) = (resources, value.as_object_mut()) {
                    object.insert("execution_resources".to_owned(), resources);
                }
                value
            } else {
                entry.value
            };
            self.append_projection(action_id, value, false)?;
        }
        let migrated = path.with_extension("migrated.json");
        let _ = fs::remove_file(&migrated);
        fs::rename(&path, &migrated)
            .map_err(|error| format!("retire legacy command projections: {error}"))?;
        Ok(())
    }
}

type ReceiptRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    String,
    i64,
    i64,
    Option<Vec<u8>>,
    Vec<u8>,
    Vec<u8>,
);

fn decode_receipt(action_id: ActionId, row: ReceiptRow) -> Result<ReconnectReceipt, String> {
    let (
        process_id,
        process_action_digest,
        replay_identity,
        prepared_digest,
        descriptor_digest,
        plan_digest,
        plan,
        creating_event,
        idempotency_key,
        interactive,
        mode,
        resource_evidence,
        protected_paths,
        stored_digest,
    ) = row;
    let resource_value = resource_evidence
        .as_deref()
        .map(|bytes| {
            let value = serde_json::from_slice::<Value>(bytes)
                .map_err(|error| format!("decode command reconnect resources: {error}"))?;
            if serde_json::to_vec(&value).map_err(|error| error.to_string())? != bytes {
                return Err("command reconnect resources are not canonical JSON".to_owned());
            }
            Ok(value)
        })
        .transpose()?;
    let receipt = ReconnectReceipt {
        action_id,
        process_id: decode_id(process_id, ProcessId::new, "process identity")?,
        process_action_digest: decode_digest(
            process_action_digest,
            "process action digest",
        )?,
        replay_identity: decode_digest(replay_identity, "replay identity")?,
        prepared_digest: decode_digest(prepared_digest, "prepared digest")?,
        descriptor_digest: SchemaDigest::new(decode_digest(
            descriptor_digest,
            "descriptor digest",
        )?),
        plan_digest: decode_digest(plan_digest, "plan digest")?,
        plan,
        creating_event: decode_id(creating_event, EventId::new, "creating event")?,
        idempotency_key,
        interactive: match interactive {
            0 => false,
            1 => true,
            _ => return Err("command reconnect interactive flag is invalid".to_owned()),
        },
        mode: decode_mode(mode)?,
        resource_evidence: resource_value,
        protected_paths: decode_paths(&protected_paths)?,
    };
    let expected = reconnect_digest(&receipt, resource_evidence.as_deref(), &protected_paths);
    if expected != decode_digest(stored_digest, "receipt digest")? {
        return Err("command reconnect receipt digest differs from its fields".to_owned());
    }
    Ok(receipt)
}

fn decode_projection(
    action_id: ActionId,
    sequence: &[u8],
    bytes: &[u8],
    digest: &[u8],
) -> Result<Value, String> {
    let sequence = decode_u64(sequence, "command projection sequence")?;
    if projection_digest(action_id, sequence, bytes)
        != decode_digest(digest.to_vec(), "command projection digest")?
    {
        return Err("projection digest differs from stored bytes".to_owned());
    }
    let value = serde_json::from_slice::<Value>(bytes)
        .map_err(|error| format!("projection JSON is malformed: {error}"))?;
    if serde_json::to_vec(&value).map_err(|error| error.to_string())? != bytes {
        return Err("projection JSON is not canonical".to_owned());
    }
    Ok(value)
}

fn reconnect_digest(
    receipt: &ReconnectReceipt,
    resource_evidence: Option<&[u8]>,
    protected_paths: &[u8],
) -> Sha256Digest {
    let mut bytes = b"peritus.command-reconnect-receipt.v1\0".to_vec();
    bytes.extend_from_slice(receipt.action_id.as_bytes());
    bytes.extend_from_slice(receipt.process_id.as_bytes());
    bytes.extend_from_slice(receipt.process_action_digest.as_bytes());
    bytes.extend_from_slice(receipt.replay_identity.as_bytes());
    bytes.extend_from_slice(receipt.prepared_digest.as_bytes());
    bytes.extend_from_slice(receipt.descriptor_digest.as_bytes());
    bytes.extend_from_slice(receipt.plan_digest.as_bytes());
    append_bytes(&mut bytes, &receipt.plan);
    bytes.extend_from_slice(receipt.creating_event.as_bytes());
    append_bytes(&mut bytes, receipt.idempotency_key.as_bytes());
    bytes.push(u8::from(receipt.interactive));
    bytes.push(u8::try_from(mode_tag(receipt.mode)).expect("mode tag is a byte"));
    match resource_evidence {
        Some(value) => {
            bytes.push(1);
            append_bytes(&mut bytes, value);
        }
        None => bytes.push(0),
    }
    append_bytes(&mut bytes, protected_paths);
    peritus_codec::sha256(&bytes)
}

fn projection_digest(action_id: ActionId, sequence: u64, value: &[u8]) -> Sha256Digest {
    let mut bytes = b"peritus.command-projection-event.v2\0".to_vec();
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    append_bytes(&mut bytes, value);
    peritus_codec::sha256(&bytes)
}

fn append_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn connect(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("open command reconnect store: {error}"))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|error| format!("configure command reconnect contention: {error}"))?;
    connection
        .pragma_update(None, "synchronous", "EXTRA")
        .map_err(|error| format!("configure command reconnect durability: {error}"))?;
    Ok(connection)
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn mode_tag(mode: CommandExecutionMode) -> i64 {
    match mode {
        CommandExecutionMode::Observational => 1,
        CommandExecutionMode::Mutation => 2,
    }
}

fn decode_mode(value: i64) -> Result<CommandExecutionMode, String> {
    match value {
        1 => Ok(CommandExecutionMode::Observational),
        2 => Ok(CommandExecutionMode::Mutation),
        _ => Err("command reconnect execution mode is invalid".to_owned()),
    }
}

fn decode_u64(bytes: &[u8], field: &str) -> Result<u64, String> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .map_err(|_| format!("{field} has an invalid width"))?;
    Ok(u64::from_be_bytes(bytes))
}

fn decode_digest(bytes: Vec<u8>, field: &str) -> Result<Sha256Digest, String> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("command reconnect {field} has an invalid width"))?;
    Ok(Sha256Digest::new(bytes))
}

fn decode_id<T, E>(
    bytes: Vec<u8>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
    field: &str,
) -> Result<T, String> {
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| format!("command reconnect {field} has an invalid width"))?;
    constructor(bytes).map_err(|_| format!("command reconnect {field} is zero"))
}

fn parse_action(handle: &str) -> Option<ActionId> {
    if handle.len() != ActionId::LENGTH * 2 {
        return None;
    }
    let mut bytes = [0_u8; ActionId::LENGTH];
    for (target, pair) in bytes.iter_mut().zip(handle.as_bytes().chunks_exact(2)) {
        *target = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    ActionId::new(bytes).ok()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn encode_paths(paths: &[PathBuf]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u64::try_from(paths.len())
            .map_err(|_| "command reconnect path count is not representable".to_owned())?
            .to_be_bytes(),
    );
    for path in paths {
        let native = native_path_bytes(path);
        append_bytes(&mut bytes, &native);
    }
    Ok(bytes)
}

fn decode_paths(mut bytes: &[u8]) -> Result<Vec<PathBuf>, String> {
    let count = take_u64(&mut bytes, "protected path count")?;
    if count > (bytes.len() / 8) as u64 {
        return Err("command reconnect protected path count exceeds its frame".to_owned());
    }
    let capacity = usize::try_from(count)
        .map_err(|_| "command reconnect path count exceeds this host".to_owned())?;
    let mut paths = Vec::new();
    paths
        .try_reserve_exact(capacity)
        .map_err(|_| "command reconnect path count cannot be allocated".to_owned())?;
    for _ in 0..count {
        let length = usize::try_from(take_u64(&mut bytes, "protected path length")?)
            .map_err(|_| "command reconnect path length exceeds this host".to_owned())?;
        if bytes.len() < length {
            return Err("command reconnect protected path frame is truncated".to_owned());
        }
        let (path, rest) = bytes.split_at(length);
        paths.push(PathBuf::from(native_path(path)?));
        bytes = rest;
    }
    if !bytes.is_empty() {
        return Err("command reconnect protected path frame has trailing bytes".to_owned());
    }
    Ok(paths)
}

fn take_u64(bytes: &mut &[u8], field: &str) -> Result<u64, String> {
    if bytes.len() < 8 {
        return Err(format!("command reconnect {field} is truncated"));
    }
    let (value, rest) = bytes.split_at(8);
    *bytes = rest;
    Ok(u64::from_be_bytes(value.try_into().expect("eight-byte slice")))
}

#[cfg(unix)]
fn native_path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn native_path(bytes: &[u8]) -> Result<OsString, String> {
    use std::os::unix::ffi::OsStringExt as _;
    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
fn native_path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt as _;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_be_bytes)
        .collect()
}

#[cfg(windows)]
fn native_path(bytes: &[u8]) -> Result<OsString, String> {
    use std::os::windows::ffi::OsStringExt as _;
    if bytes.len() % 2 != 0 {
        return Err("command reconnect Windows path has an odd byte length".to_owned());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Ok(OsString::from_wide(&units))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyProjectionFile {
    version: u16,
    entries: Vec<LegacyProjection>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyProjection {
    handle: String,
    value: Value,
}

fn quarantine(path: &Path) {
    let quarantine = path.with_extension("corrupt.json");
    let _ = fs::remove_file(&quarantine);
    if let Err(error) = fs::rename(path, &quarantine) {
        crate::diagnostic::report(&format!(
            "peritus command runtime: could not quarantine malformed handle projections {}: {error}",
            path.display()
        ));
    }
}
