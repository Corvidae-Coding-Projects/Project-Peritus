//! Reusable document bytes with sparse line offsets. Access is checked by each caller.

use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Cursor, Read as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use peritus_agent::DeveloperLoopError;

use super::{inspection_cancellation::InspectionCancellation, path::tool};
use crate::file_observation::{Fingerprint, fingerprint};

// These are retention bounds, never document/read limits. Larger inputs stream normally.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const LINE_STRIDE: usize = 128;

struct Document {
    bytes: Arc<[u8]>,
    offsets: Vec<usize>,
}

impl Document {
    fn reader(&self, start: usize) -> (Box<dyn BufRead>, usize) {
        let block = start.saturating_sub(1) / LINE_STRIDE;
        let offset = self.offsets.get(block).copied().unwrap_or(self.bytes.len());
        let mut cursor = Cursor::new(Arc::clone(&self.bytes));
        cursor.set_position(offset as u64);
        (Box::new(cursor), block * LINE_STRIDE + 1)
    }

    fn size(&self) -> usize {
        self.bytes.len().saturating_add(self.offsets.len() * size_of::<usize>()).saturating_add(512)
    }
}

struct Entry {
    token: Fingerprint,
    document: Arc<Document>,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: BTreeMap<PathBuf, Entry>,
    bytes: usize,
    clock: u64,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

pub(super) fn reader(
    path: &Path,
    start: usize,
    cancellation: &InspectionCancellation,
) -> Result<(Box<dyn BufRead>, usize), DeveloperLoopError> {
    cancellation.check()?;
    let before = fs::symlink_metadata(path).map_err(|error| tool(error.to_string()))?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(tool("document observation requires a regular file"));
    }
    // Reopen even on a hit: a cached body must not bypass current OS access checks.
    let mut file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    let opened = file.metadata().map_err(|error| tool(error.to_string()))?;
    let Some(token) =
        fingerprint(&before).filter(|token| fingerprint(&opened).as_ref() == Some(token))
    else {
        return Ok((Box::new(BufReader::new(file)), 1));
    };
    if let Ok(mut cache) = cache().lock() {
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        if let Some(entry) = cache.entries.get_mut(path)
            && entry.token == token
        {
            entry.used = used;
            return Ok(entry.document.reader(start));
        }
        if let Some(old) = cache.entries.remove(path) {
            cache.bytes = cache.bytes.saturating_sub(old.document.size());
        }
    }
    if opened.len() > ENTRY_BYTES {
        return Ok((Box::new(BufReader::new(file)), 1));
    }
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        cancellation.check()?;
        let count = file.read(&mut buffer).map_err(|error| tool(error.to_string()))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() as u64 > ENTRY_BYTES {
            // A growing file can exceed the retention preference without becoming unreadable.
            return Ok((
                Box::new(BufReader::new(
                    fs::File::open(path).map_err(|error| tool(error.to_string()))?,
                )),
                1,
            ));
        }
    }
    if fingerprint(&file.metadata().map_err(|error| tool(error.to_string()))?).as_ref()
        != Some(&token)
        || fingerprint(&fs::symlink_metadata(path).map_err(|error| tool(error.to_string()))?)
            .as_ref()
            != Some(&token)
    {
        return Err(tool("file changed during observation; reread this target"));
    }
    let mut offsets = vec![0];
    let mut line = 0_usize;
    for (offset, byte) in bytes.iter().enumerate() {
        if offset % (64 * 1024) == 0 {
            cancellation.check()?;
        }
        if *byte == b'\n' {
            line += 1;
            if line.is_multiple_of(LINE_STRIDE) {
                offsets.push(offset + 1);
            }
        }
    }
    let document = Arc::new(Document { bytes: bytes.into(), offsets });
    let size = document.size();
    if let Ok(mut cache) = cache().lock() {
        if let Some(old) = cache.entries.remove(path) {
            cache.bytes = cache.bytes.saturating_sub(old.document.size());
        }
        while cache.bytes.saturating_add(size) > CACHE_BYTES {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone());
            let Some(oldest) = oldest else { break };
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes = cache.bytes.saturating_sub(old.document.size());
            }
        }
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        cache
            .entries
            .insert(path.to_owned(), Entry { token, document: Arc::clone(&document), used });
        cache.bytes = cache.bytes.saturating_add(size);
    }
    Ok(document.reader(start))
}
