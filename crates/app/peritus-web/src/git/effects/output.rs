//! Complete durable streams with bounded physical projections.

use super::{
    EffectResult, Phase, Request, StoreBinding, digest, read_json, read_state, validate_request,
    validate_state,
};
use crate::{
    error::{Result, problem, uncertain},
    state::{hex, save},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const OUTPUT_PAGE_BYTES: usize = 64 * 1_024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::git) struct Capture {
    pub(super) bytes: u64,
    pub(super) digest: String,
    pub(super) pages: u64,
    pub(super) page_root: String,
    pub(super) complete: bool,
    pub(super) error: Option<String>,
}

impl Capture {
    pub(super) fn empty() -> Self {
        Self {
            bytes: 0,
            digest: digest(&[]),
            pages: 0,
            page_root: empty_page_root(),
            complete: true,
            error: None,
        }
    }

    pub(super) fn failed(error: impl Into<String>) -> Self {
        Self {
            bytes: 0,
            digest: digest(&[]),
            pages: 0,
            page_root: empty_page_root(),
            complete: false,
            error: Some(error.into()),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PageReceipt {
    offset: u64,
    bytes: u64,
    digest: String,
    proof: Vec<MerkleStep>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MerkleStep {
    digest: String,
    left: bool,
}

pub(super) fn capture(mut source: impl Read, path: &Path) -> Capture {
    let mut target = match OpenOptions::new().create_new(true).write(true).open(path) {
        Ok(target) => Some(target),
        Err(error) => return drain_after_failure(&mut source, error.to_string()),
    };
    let pages_path = page_directory(path);
    if let Err(error) = std::fs::create_dir(&pages_path) {
        return drain_after_failure(&mut source, error.to_string());
    }
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut page_digests = Vec::<[u8; 32]>::new();
    let mut error = None;
    loop {
        let mut chunk = Vec::with_capacity(OUTPUT_PAGE_BYTES);
        let read = match source
            .by_ref()
            .take(u64::try_from(OUTPUT_PAGE_BYTES).unwrap_or(u64::MAX))
            .read_to_end(&mut chunk)
        {
            Ok(0) => break,
            Ok(read) => read,
            Err(failure) => {
                error.get_or_insert_with(|| failure.to_string());
                break;
            }
        };
        digest.update(&chunk);
        match bytes.checked_add(u64::try_from(read).unwrap_or(u64::MAX)) {
            Some(total) => bytes = total,
            None => {
                error.get_or_insert_with(|| "Git output length exceeded its receipt range".into());
                target = None;
            }
        }
        if let Some(file) = target.as_mut()
            && let Err(failure) = file.write_all(&chunk)
        {
            error.get_or_insert_with(|| failure.to_string());
            target = None;
        }
        page_digests.push(Sha256::digest(&chunk).into());
    }
    if let Some(file) = target.as_mut()
        && let Err(failure) = file.sync_all()
    {
        error.get_or_insert_with(|| failure.to_string());
    }
    let page_count = page_digests.len();
    let page_bytes = u64::try_from(OUTPUT_PAGE_BYTES).unwrap_or(u64::MAX);
    let leaves = page_digests
        .iter()
        .enumerate()
        .map(|(index, content)| {
            let index_u64 = u64::try_from(index).unwrap_or(u64::MAX);
            let offset = index_u64.saturating_mul(page_bytes);
            let length = bytes.saturating_sub(offset).min(page_bytes);
            leaf_digest(index_u64, u64::try_from(page_count).unwrap_or(u64::MAX), offset, length, content)
        })
        .collect();
    let levels = merkle_levels(leaves);
    let page_root = levels
        .last()
        .and_then(|level| level.first())
        .map_or_else(empty_page_root, |root| hex(root));
    if target.is_some() {
        for index in 0..levels.first().map_or(0, Vec::len) {
            let index_u64 = match u64::try_from(index) {
                Ok(index) => index,
                Err(failure) => {
                    error.get_or_insert_with(|| failure.to_string());
                    target = None;
                    break;
                }
            };
            let offset = match index_u64
                .checked_mul(u64::try_from(OUTPUT_PAGE_BYTES).unwrap_or(u64::MAX))
            {
                Some(offset) => offset,
                None => {
                    error.get_or_insert_with(|| "Git output page offset overflow".into());
                    target = None;
                    break;
                }
            };
            let receipt = PageReceipt {
                offset,
                bytes: bytes
                    .saturating_sub(offset)
                    .min(u64::try_from(OUTPUT_PAGE_BYTES).unwrap_or(u64::MAX)),
                digest: hex(&page_digests[index]),
                proof: merkle_proof(&levels, index),
            };
            if let Err(failure) = serde_json::to_vec(&receipt)
                .map_err(problem)
                .and_then(|encoded| save(&page_receipt_path(&pages_path, index_u64), &encoded))
            {
                error.get_or_insert(failure.0);
                target = None;
                break;
            }
        }
    }
    if let Err(failure) = sync_directory(&pages_path) {
        error.get_or_insert(failure.0);
    }
    Capture {
        bytes,
        digest: hex(&digest.finalize()),
        pages: u64::try_from(levels.first().map_or(0, Vec::len)).unwrap_or(u64::MAX),
        page_root,
        complete: error.is_none() && target.is_some(),
        error,
    }
}

fn drain_after_failure(source: &mut impl Read, failure: String) -> Capture {
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut error = failure;
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                digest.update(&buffer[..read]);
                bytes = bytes.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
            }
            Err(failure) => {
                error.push_str("; stream drain failed: ");
                error.push_str(&failure.to_string());
                break;
            }
        }
    }
    Capture {
        bytes,
        digest: hex(&digest.finalize()),
        pages: 0,
        page_root: empty_page_root(),
        complete: false,
        error: Some(error),
    }
}

pub(super) fn projection(
    directory: &Path,
    operation: &str,
    result: &EffectResult,
) -> Result<Value> {
    Ok(json!({
        "operation":operation,
        "status":result.status,
        "stdout":page(directory, "stdout", &result.stdout, 0)?,
        "stderr":page(directory, "stderr", &result.stderr, 0)?,
    }))
}

pub(super) fn error_detail(
    directory: &Path,
    result: &EffectResult,
) -> Result<Option<String>> {
    for (stream, receipt) in [("stderr", &result.stderr), ("stdout", &result.stdout)] {
        if receipt.bytes == 0 || !receipt.complete {
            continue;
        }
        let (bytes, _) = read_page(directory, stream, receipt, 0)?;
        let mut detail = String::from_utf8_lossy(&bytes).into_owned();
        if receipt.bytes > u64::try_from(bytes.len()).unwrap_or(0) {
            detail.push_str(&format!(
                "\n[{stream} continues in the retained output; {} bytes total, sha256 {}]",
                receipt.bytes, receipt.digest,
            ));
        }
        return Ok(Some(detail));
    }
    Ok(None)
}

pub(super) fn validate(directory: &Path, stream: &str, receipt: &Capture) -> Result<()> {
    if !receipt.complete {
        return Err(uncertain(format!("Git {stream} was not retained completely")));
    }
    let mut file = std::fs::File::open(directory.join(stream))?;
    if file.metadata()?.len() != receipt.bytes {
        return Err(uncertain("Retained Git output length does not match its terminal receipt"));
    }
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        bytes = bytes
            .checked_add(u64::try_from(read).map_err(problem)?)
            .ok_or_else(|| problem("Git output length overflow"))?;
    }
    if bytes != receipt.bytes || hex(&digest.finalize()) != receipt.digest {
        return Err(uncertain("Retained Git output does not match its terminal receipt"));
    }
    let expected_pages = if receipt.bytes == 0 {
        0
    } else {
        receipt
            .bytes
            .saturating_add(u64::try_from(OUTPUT_PAGE_BYTES - 1).map_err(problem)?)
            / u64::try_from(OUTPUT_PAGE_BYTES).map_err(problem)?
    };
    if receipt.pages != expected_pages {
        return Err(uncertain("Retained Git output has an invalid sealed-page count"));
    }
    if receipt.page_root.len() != 64
        || !receipt.page_root.bytes().all(|byte| byte.is_ascii_hexdigit())
        || (receipt.pages == 0 && receipt.page_root != empty_page_root())
    {
        return Err(uncertain("Retained Git output has an invalid sealed-page root"));
    }
    for index in 0..receipt.pages {
        let offset = index
            .checked_mul(u64::try_from(OUTPUT_PAGE_BYTES).map_err(problem)?)
            .ok_or_else(|| problem("Git output page offset overflow"))?;
        let _ = read_page(directory, stream, receipt, offset)?;
    }
    Ok(())
}

pub(super) fn page_for_operation(
    store: &StoreBinding,
    operation: &str,
    prepared: Option<&Value>,
    stream: &str,
    offset: u64,
) -> Result<Value> {
    let directory = store.directory(operation);
    let request: Request = read_json(&directory.join("request.json"))?;
    validate_request(store, &request, prepared)?;
    if request.operation != operation {
        return Err(uncertain("The Git output belongs to another operation"));
    }
    let state = read_state(&directory)?
        .ok_or_else(|| uncertain("The Git output owner has not published its state"))?;
    validate_state(&request, &state)?;
    if state.phase != Phase::Completed {
        return Err(problem("The Git output is still being written"));
    }
    let result = state
        .result
        .as_ref()
        .ok_or_else(|| uncertain("The completed Git effect has no output receipt"))?;
    let receipt = match stream {
        "stdout" => &result.stdout,
        "stderr" => &result.stderr,
        _ => return Err(problem("Choose stdout or stderr")),
    };
    page(&directory, stream, receipt, offset)
}

fn page(directory: &Path, stream: &str, receipt: &Capture, offset: u64) -> Result<Value> {
    if !receipt.complete {
        return Ok(json!({
            "stream":stream,
            "offset":offset.to_string(),
            "previous":Value::Null,
            "next":Value::Null,
            "bytes":receipt.bytes.to_string(),
            "digest":receipt.digest,
            "pageBytes":"0",
            "pageDigest":digest(&[]),
            "data":"",
            "error":receipt.error,
        }));
    }
    if receipt.bytes == 0 && offset == 0 {
        return Ok(json!({
            "stream":stream,"offset":"0","previous":Value::Null,"next":Value::Null,
            "bytes":"0","digest":receipt.digest,"pageBytes":"0",
            "pageDigest":digest(&[]),"data":""
        }));
    }
    let (bytes, sealed) = read_page(directory, stream, receipt, offset)?;
    let length = u64::try_from(bytes.len()).map_err(problem)?;
    let end = offset.checked_add(length).ok_or_else(|| problem("Git output cursor overflow"))?;
    let next = (end < receipt.bytes).then_some(end);
    let previous = (offset != 0).then_some(
        offset.saturating_sub(u64::try_from(OUTPUT_PAGE_BYTES).map_err(problem)?),
    );
    Ok(json!({
        "stream":stream,
        "offset":offset.to_string(),
        "next":next.map(|value|value.to_string()),
        "previous":previous.map(|value|value.to_string()),
        "bytes":receipt.bytes.to_string(),
        "digest":receipt.digest,
        "pageBytes":length.to_string(),
        "pageDigest":sealed.digest,
        "data":STANDARD.encode(bytes),
    }))
}

fn read_page(
    directory: &Path,
    stream: &str,
    receipt: &Capture,
    offset: u64,
) -> Result<(Vec<u8>, PageReceipt)> {
    let page_bytes = u64::try_from(OUTPUT_PAGE_BYTES).map_err(problem)?;
    if offset >= receipt.bytes || offset % page_bytes != 0 {
        return Err(problem("The Git output cursor is past the retained stream"));
    }
    let index = offset / page_bytes;
    if index >= receipt.pages {
        return Err(uncertain("The Git output page has no sealed receipt"));
    }
    let receipt_path = page_receipt_path(&page_directory(&directory.join(stream)), index);
    let sealed: PageReceipt = serde_json::from_slice(&std::fs::read(receipt_path)?)?;
    let expected_bytes = receipt.bytes.saturating_sub(offset).min(page_bytes);
    if sealed.offset != offset
        || sealed.bytes == 0
        || sealed.bytes != expected_bytes
        || sealed.offset.checked_add(sealed.bytes).is_none_or(|end| end > receipt.bytes)
        || sealed.digest.len() != 64
        || !sealed.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !verify_merkle(&sealed, &receipt.page_root, index, receipt.pages)
    {
        return Err(uncertain("The Git output page receipt is malformed"));
    }
    let mut file = std::fs::File::open(directory.join(stream))?;
    if file.metadata()?.len() != receipt.bytes {
        return Err(uncertain("Retained Git output length does not match its terminal receipt"));
    }
    file.seek(SeekFrom::Start(offset))?;
    let allocation = usize::try_from(sealed.bytes).map_err(problem)?;
    let mut bytes = vec![0_u8; allocation];
    file.read_exact(&mut bytes)?;
    if hex(&Sha256::digest(&bytes)) != sealed.digest {
        return Err(uncertain("The Git output page changed after it was sealed"));
    }
    Ok((bytes, sealed))
}

fn page_directory(path: &Path) -> std::path::PathBuf {
    path.with_file_name(format!("{}.pages", path.file_name().unwrap_or_default().to_string_lossy()))
}

fn page_receipt_path(directory: &Path, index: u64) -> std::path::PathBuf {
    directory.join(format!("{index:020}.json"))
}

fn merkle_levels(leaves: Vec<[u8; 32]>) -> Vec<Vec<[u8; 32]>> {
    if leaves.is_empty() {
        return Vec::new();
    }
    let mut levels = vec![leaves];
    while levels.last().is_some_and(|level| level.len() > 1) {
        let previous = levels.last().expect("Merkle level exists");
        let mut next = Vec::with_capacity(previous.len().div_ceil(2));
        for pair in previous.chunks(2) {
            next.push(if pair.len() == 2 { node_digest(&pair[0], &pair[1]) } else { pair[0] });
        }
        levels.push(next);
    }
    levels
}

fn merkle_proof(levels: &[Vec<[u8; 32]>], mut index: usize) -> Vec<MerkleStep> {
    let mut proof = Vec::new();
    for level in levels.iter().take(levels.len().saturating_sub(1)) {
        let sibling = if index % 2 == 0 { index.checked_add(1) } else { index.checked_sub(1) };
        if let Some(sibling) = sibling.and_then(|sibling| level.get(sibling)) {
            proof.push(MerkleStep { digest: hex(sibling), left: index % 2 == 1 });
        }
        index /= 2;
    }
    proof
}

fn verify_merkle(receipt: &PageReceipt, root: &str, mut index: u64, mut pages: u64) -> bool {
    let Some(content) = parse_digest(&receipt.digest) else { return false };
    if pages == 0 || index >= pages {
        return false;
    }
    let mut digest = leaf_digest(index, pages, receipt.offset, receipt.bytes, &content);
    let mut proof = receipt.proof.iter();
    while pages > 1 {
        let sibling_index = index ^ 1;
        if sibling_index < pages {
            let Some(step) = proof.next() else { return false };
            if step.left != (index % 2 == 1) {
                return false;
            }
            let Some(sibling) = parse_digest(&step.digest) else { return false };
            digest = if step.left {
                node_digest(&sibling, &digest)
            } else {
                node_digest(&digest, &sibling)
            };
        }
        index /= 2;
        pages = pages.div_ceil(2);
    }
    proof.next().is_none() && hex(&digest) == root
}

fn leaf_digest(
    index: u64,
    pages: u64,
    offset: u64,
    bytes: u64,
    content: &[u8; 32],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"peritus-web-git-output-page-leaf-v2\0");
    digest.update(index.to_be_bytes());
    digest.update(pages.to_be_bytes());
    digest.update(offset.to_be_bytes());
    digest.update(bytes.to_be_bytes());
    digest.update(content);
    digest.finalize().into()
}

fn node_digest(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"peritus-web-git-output-page-node-v1\0");
    digest.update(left);
    digest.update(right);
    digest.finalize().into()
}

fn empty_page_root() -> String {
    digest(b"peritus-web-git-output-pages-empty-v1\0")
}

fn parse_digest(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(value.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(bytes)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)?.sync_all().map_err(uncertain)
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<()> {
    let _ = std::fs::metadata(path)?;
    Ok(())
}
