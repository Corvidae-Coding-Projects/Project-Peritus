//! Durable frame loading, truncated-tail recovery, and append-only persistence.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{EffectReceiptLedger, ReceiptRecord, ReceiptState, codec, fields_are_consistent, tool};

impl EffectReceiptLedger {
    pub(super) fn load(&mut self) -> Result<(), DeveloperLoopError> {
        if self.loaded {
            return Ok(());
        }
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.loaded = true;
                return Ok(());
            }
            Err(error) => return Err(tool(format!("read effect receipts: {error}"))),
        };
        if !file
            .metadata()
            .map_err(|error| tool(format!("inspect effect receipts: {error}")))?
            .is_file()
        {
            return Err(tool("read effect receipts: ledger path is not a regular file"));
        }
        let (records, offset) = decode_records(&mut file)?;
        for record in records {
            self.accept_loaded(record);
        }
        let retained = file
            .metadata()
            .map_err(|error| tool(format!("inspect effect receipts: {error}")))?
            .len();
        if offset != retained {
            drop(file);
            let file = OpenOptions::new().write(true).open(&self.path).map_err(|error| {
                tool(format!("open effect receipts for tail recovery: {error}"))
            })?;
            file.set_len(offset)
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
        let length = u64::try_from(payload.len())
            .map_err(|_| tool("effect receipt frame exceeds this platform"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| tool(format!("create effect receipt directory: {error}")))?;
        }
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
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(tool(format!("read effect receipts: {error}"))),
    };
    decode_records(&mut file).map(|(records, _)| records)
}

fn decode_records(
    reader: &mut impl io::Read,
) -> Result<(Vec<ReceiptRecord>, u64), DeveloperLoopError> {
    let mut latest = BTreeMap::<(String, u32), ReceiptRecord>::new();
    let mut offset = 0_u64;
    while let Some(length_bytes) = read_frame_header(reader)? {
        let length = u64::from_le_bytes(length_bytes);
        let payload_length = usize::try_from(length)
            .map_err(|_| tool("effect receipt length exceeds this platform"))?;
        let mut payload = Vec::new();
        let actual_length = reader
            .take(length)
            .read_to_end(&mut payload)
            .map_err(|error| tool(format!("read effect receipt frame: {error}")))?;
        if actual_length != payload_length {
            break;
        }
        let value: Value = serde_json::from_slice(&payload)
            .map_err(|error| tool(format!("decode effect receipt: {error}")))?;
        let mut record = codec::decode(&value)?;
        if record.state == ReceiptState::Completed && record.output.is_none() {
            let key = (record.scope.clone(), record.ordinal);
            let previous = latest
                .get(&key)
                .ok_or_else(|| tool("completed effect receipt has no applied result"))?;
            if previous.output.is_none() || previous.is_error.is_none() {
                return Err(tool("completed effect receipt lost its prior result"));
            }
            record.output.clone_from(&previous.output);
            record.is_error = previous.is_error;
        }
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
        let frame_length = 8_u64
            .checked_add(length)
            .ok_or_else(|| tool("effect receipt frame length overflowed"))?;
        offset = offset
            .checked_add(frame_length)
            .ok_or_else(|| tool("effect receipt ledger offset overflowed"))?;
        latest.insert(key, record);
    }
    Ok((latest.into_values().collect(), offset))
}

fn read_frame_header(reader: &mut impl io::Read) -> Result<Option<[u8; 8]>, DeveloperLoopError> {
    let mut bytes = [0_u8; 8];
    let mut read = 0;
    while read < bytes.len() {
        match reader.read(&mut bytes[read..]) {
            Ok(0) => return Ok(None),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(tool(format!("read effect receipt header: {error}"))),
        }
    }
    Ok(Some(bytes))
}
