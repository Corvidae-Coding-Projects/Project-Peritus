//! Durable frame loading, truncated-tail recovery, and append-only persistence.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    EffectReceiptLedger, FORMAT_VERSION, MAX_LEDGER_BYTES, MAX_RECORD_BYTES, ReceiptRecord, codec,
    fields_are_consistent, tool,
};

impl EffectReceiptLedger {
    pub(super) fn load(&mut self) -> Result<(), DeveloperLoopError> {
        if self.loaded {
            return Ok(());
        }
        let bytes = read_bytes(&self.path)?;
        let (records, offset) = decode_records(&bytes)?;
        for record in records {
            self.accept_loaded(record);
        }
        if offset != bytes.len() {
            let file = OpenOptions::new().write(true).open(&self.path).map_err(|error| {
                tool(format!("open effect receipts for tail recovery: {error}"))
            })?;
            file.set_len(
                u64::try_from(offset)
                    .map_err(|_| tool("effect receipt recovery offset exceeds this platform"))?,
            )
            .and_then(|()| file.sync_data())
            .map_err(|error| tool(format!("recover truncated effect receipt tail: {error}")))?;
        }
        self.loaded = true;
        Ok(())
    }

    fn accept_loaded(&mut self, record: ReceiptRecord) {
        let key = (record.scope.clone(), record.ordinal);
        self.all_entries.insert(key, record.clone());
        if record.scope == self.scope {
            self.entries.insert(record.ordinal, record);
        }
    }

    pub(super) fn append(&self, record: &ReceiptRecord) -> Result<(), DeveloperLoopError> {
        let payload = serde_json::to_vec(&codec::encode(record))
            .map_err(|error| tool(format!("encode effect receipt: {error}")))?;
        if payload.len() > MAX_RECORD_BYTES {
            return Err(tool("effect receipt result exceeds its byte bound"));
        }
        let frame_bytes = payload
            .len()
            .checked_add(8)
            .ok_or_else(|| tool("effect receipt frame length overflowed"))?;
        let retained =
            fs::metadata(&self.path)
                .map(|metadata| metadata.len())
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound { Ok(0) } else { Err(error) }
                })
                .map_err(|error| tool(format!("inspect effect receipts: {error}")))?;
        let frame_bytes = u64::try_from(frame_bytes)
            .map_err(|_| tool("effect receipt frame exceeds this platform"))?;
        if retained.checked_add(frame_bytes).is_none_or(|total| total > MAX_LEDGER_BYTES as u64) {
            return Err(tool("effect receipt ledger exceeds its byte bound"));
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| tool(format!("create effect receipt directory: {error}")))?;
        }
        let length = u64::try_from(payload.len())
            .map_err(|_| tool("effect receipt result exceeds this platform"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| tool(format!("open effect receipts: {error}")))?;
        file.write_all(&length.to_le_bytes())
            .and_then(|()| file.write_all(&payload))
            .and_then(|()| file.sync_data())
            .map_err(|error| tool(format!("persist effect receipt: {error}")))
    }
}

pub(super) fn inspect(path: &Path) -> Result<Vec<ReceiptRecord>, DeveloperLoopError> {
    let bytes = read_bytes(path)?;
    decode_records(&bytes).map(|(records, _)| records)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, DeveloperLoopError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(tool(format!("read effect receipts: {error}"))),
    };
    if bytes.len() > MAX_LEDGER_BYTES {
        return Err(tool("effect receipt ledger exceeds its byte bound"));
    }
    Ok(bytes)
}

fn decode_records(bytes: &[u8]) -> Result<(Vec<ReceiptRecord>, usize), DeveloperLoopError> {
    let mut latest = BTreeMap::<(String, u32), ReceiptRecord>::new();
    let mut offset = 0_usize;
    while bytes.len().saturating_sub(offset) >= 8 {
        let length = u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .map_err(|_| tool("effect receipt length is malformed"))?,
        );
        let length = usize::try_from(length)
            .map_err(|_| tool("effect receipt length exceeds this platform"))?;
        if length > MAX_RECORD_BYTES {
            return Err(tool("effect receipt record exceeds its byte bound"));
        }
        let start = offset + 8;
        let end =
            start.checked_add(length).ok_or_else(|| tool("effect receipt length overflowed"))?;
        if end > bytes.len() {
            break;
        }
        let value: Value = serde_json::from_slice(&bytes[start..end])
            .map_err(|error| tool(format!("decode effect receipt: {error}")))?;
        let record = codec::decode(&value)?;
        if record.version == FORMAT_VERSION {
            if !fields_are_consistent(&record) {
                return Err(tool("effect receipt state fields are inconsistent"));
            }
            let key = (record.scope.clone(), record.ordinal);
            if let Some(previous) = latest.get(&key)
                && (previous.tool != record.tool
                    || previous.request_sha256 != record.request_sha256
                    || previous.call_id != record.call_id)
            {
                return Err(tool("effect receipt history contains a conflicting action identity"));
            }
            latest.insert(key, record);
        }
        offset = end;
    }
    Ok((latest.into_values().collect(), offset))
}
