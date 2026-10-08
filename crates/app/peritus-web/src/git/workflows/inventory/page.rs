use super::{
    InventoryPage, InventoryStore, MaterializationPhase, MaterializationRequest,
    MaterializationState, PAGE_RECORDS, PageReceipt, SCHEMA_VERSION, validate_state,
};
use crate::{
    error::{Result, problem, uncertain},
    git::effects,
    state::hex,
};
use peritus_process::ProbeObservation;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(40);

pub(super) async fn read(
    store: &InventoryStore,
    snapshot: &str,
    index: u64,
    mut launched: Option<&mut super::materialize::Launched>,
) -> Result<InventoryPage> {
    let directory = store.directory(snapshot)?;
    let request = read_request(store, &directory)?;
    let receipt = loop {
        if let Some(receipt) = read_receipt(&directory, index)? {
            validate_receipt(&directory, &request, &receipt, index)?;
            break receipt;
        }
        let state = read_state(&directory)?;
        if let Some(state) = state.as_ref() {
            validate_state(&request, state)?;
            if state.phase == MaterializationPhase::Completed && state.pages == index {
                return Ok(empty_page(snapshot));
            }
            terminal_without_page(state, index)?;
            ensure_running_owner(state)?;
        }
        if let Some(child) = launched.as_deref_mut()
            && let Some(status) = child.try_wait()?
        {
            if let Some(state) = read_state(&directory)? {
                validate_state(&request, &state)?;
                terminal_without_page(&state, index)?;
            }
            return Err(uncertain(format!(
                "The Git inventory owner stopped before publishing page {index} ({status})",
            )));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };

    let next = index.checked_add(1).ok_or_else(|| problem("Git inventory cursor overflow"))?;
    let cursor = if let Some(next_receipt) = read_receipt(&directory, next)? {
        validate_receipt(&directory, &request, &next_receipt, next)?;
        Some(next.to_string())
    } else {
        let state = read_state(&directory)?
            .ok_or_else(|| uncertain("The Git inventory owner has no durable state"))?;
        validate_state(&request, &state)?;
        match state.phase {
            MaterializationPhase::Running => {
                ensure_running_owner(&state)?;
                Some(next.to_string())
            }
            MaterializationPhase::Failed => {
                return Err(problem(state.error.unwrap_or_else(|| {
                    "Git inventory materialization failed".into()
                })))
            }
            MaterializationPhase::Completed
                if state.pages == next
                    && state.bytes == receipt.offset.saturating_add(receipt.bytes) => None,
            MaterializationPhase::Completed => {
                return Err(uncertain("A durable Git inventory page receipt is missing"));
            }
        }
    };
    read_page(&directory, snapshot, receipt, cursor)
}

fn empty_page(snapshot: &str) -> InventoryPage {
    InventoryPage {
        branches: Vec::new(), remotes: Vec::new(), summary: String::new(), cursor: None,
        snapshot: snapshot.to_owned(),
    }
}

fn read_page(
    directory: &Path,
    snapshot: &str,
    receipt: PageReceipt,
    cursor: Option<String>,
) -> Result<InventoryPage> {
    let mut file = File::open(directory.join("records.jsonl"))?;
    let end = receipt
        .offset
        .checked_add(receipt.bytes)
        .ok_or_else(|| problem("Git inventory page length overflow"))?;
    if file.metadata()?.len() < end {
        return Err(uncertain("The Git inventory page is shorter than its durable receipt"));
    }
    file.seek(SeekFrom::Start(receipt.offset))?;
    let mut bytes = vec![0_u8; usize::try_from(receipt.bytes).map_err(problem)?];
    file.read_exact(&mut bytes)?;
    if hex(&Sha256::digest(&bytes)) != receipt.digest {
        return Err(uncertain("The Git inventory page changed after publication"));
    }
    let mut branches = Vec::new();
    let mut remotes = BTreeMap::<String, (Vec<String>, Vec<String>)>::new();
    let mut observed_records = 0_usize;
    for record in bytes.split_inclusive(|byte| *byte == b'\n') {
        if record.last().copied() != Some(b'\n') {
            return Err(uncertain("A materialized Git inventory record is malformed"));
        }
        observed_records += 1;
        let value: Value = serde_json::from_slice(record)?;
        match value["kind"].as_str() {
            Some("branch") => branches.push(value),
            Some("remote") => merge_remote(&mut remotes, &value)?,
            _ => return Err(uncertain("The Git inventory contains an unknown record kind")),
        }
    }
    if observed_records != receipt.records {
        return Err(uncertain("The Git inventory page record count changed"));
    }
    let mut summary = String::new();
    let remotes = remotes
        .into_iter()
        .map(|(name, (fetch, push))| {
            for value in &fetch { summary.push_str(&format!("{name}\t{value} (fetch)\n")); }
            for value in if push.is_empty() { &fetch } else { &push } {
                summary.push_str(&format!("{name}\t{value} (push)\n"));
            }
            json!({"name":name,"fetch":fetch,"push":push})
        })
        .collect();
    Ok(InventoryPage {
        branches,
        remotes,
        summary,
        cursor,
        snapshot: snapshot.to_owned(),
    })
}

fn merge_remote(
    remotes: &mut BTreeMap<String, (Vec<String>, Vec<String>)>,
    value: &Value,
) -> Result<()> {
    let name = value["name"]
        .as_str()
        .ok_or_else(|| uncertain("A materialized remote has no name"))?
        .to_owned();
    let remote = remotes.entry(name).or_default();
    for url in value["fetch"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if !remote.0.iter().any(|known| known == url) { remote.0.push(url.to_owned()); }
    }
    for url in value["push"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if !remote.1.iter().any(|known| known == url) { remote.1.push(url.to_owned()); }
    }
    Ok(())
}

fn terminal_without_page(state: &MaterializationState, index: u64) -> Result<()> {
    match state.phase {
        MaterializationPhase::Running => Ok(()),
        MaterializationPhase::Failed => Err(problem(state.error.clone().unwrap_or_else(|| {
            "Git inventory materialization failed".into()
        }))),
        MaterializationPhase::Completed => Err(problem(if state.pages == 0 && index == 0 {
            "The Git inventory is empty"
        } else {
            "The Git inventory cursor is past its materialization"
        })),
    }
}

fn ensure_running_owner(state: &MaterializationState) -> Result<()> {
    if state.phase != MaterializationPhase::Running {
        return Ok(());
    }
    match effects::observe_detached_owner(&state.owner)? {
        ProbeObservation::ExactLive => Ok(()),
        ProbeObservation::ExactAbsent => Err(uncertain(
            "The Git inventory owner stopped without publishing a terminal state",
        )),
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => Err(uncertain(
            "The exact Git inventory owner can no longer be verified",
        )),
    }
}

fn read_request(store: &InventoryStore, directory: &Path) -> Result<MaterializationRequest> {
    let request: MaterializationRequest =
        serde_json::from_slice(&std::fs::read(directory.join("request.json"))?)?;
    store.validate_request(&request, directory)?;
    Ok(request)
}

fn read_state(directory: &Path) -> Result<Option<MaterializationState>> {
    read_optional(&directory.join("state.json"))
}

fn read_receipt(directory: &Path, index: u64) -> Result<Option<PageReceipt>> {
    read_optional(&directory.join("pages").join(format!("{index:020}.json")))
}

fn read_optional<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_receipt(
    directory: &Path,
    request: &MaterializationRequest,
    receipt: &PageReceipt,
    index: u64,
) -> Result<()> {
    if receipt.schema_version != SCHEMA_VERSION
        || receipt.snapshot != request.snapshot
        || receipt.index != index
        || receipt.bytes == 0
        || receipt.records == 0
        || receipt.records > PAGE_RECORDS
        || (index == 0 && receipt.offset != 0)
        || receipt.digest.len() != 64
        || !receipt.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(uncertain("The Git inventory page receipt is malformed"));
    }
    if index != 0 {
        let previous_index = index - 1;
        let previous = read_receipt(directory, previous_index)?
            .ok_or_else(|| uncertain("The preceding Git inventory page receipt is missing"))?;
        if previous.schema_version != SCHEMA_VERSION
            || previous.snapshot != request.snapshot
            || previous.index != previous_index
            || previous.offset.checked_add(previous.bytes) != Some(receipt.offset)
        {
            return Err(uncertain("The Git inventory page index is not contiguous"));
        }
    }
    Ok(())
}
