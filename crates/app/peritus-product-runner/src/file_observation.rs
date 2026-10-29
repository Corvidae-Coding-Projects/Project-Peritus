//! Reusable process-local content observations. Callers retain path and authority checks.

use std::{
    collections::BTreeMap,
    fs::{self, Metadata},
    io::Read as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use sha2::{Digest as _, Sha256};

const PREVIEW_BYTES: usize = 16 * 1024;
const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct ContentObservation {
    pub(crate) digest: [u8; 32],
    pub(crate) bytes: u64,
    pub(crate) preview: Vec<u8>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct Fingerprint(Vec<u64>);

impl Fingerprint {
    /// Versioned identity of the existing platform change token, not proof of a candidate.
    pub(crate) fn binding(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"peritus-file-observation-token-v1\0");
        for field in &self.0 {
            digest.update(field.to_be_bytes());
        }
        digest.finalize().into()
    }
}

struct Cached {
    fingerprint: Fingerprint,
    content: Arc<ContentObservation>,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: BTreeMap<PathBuf, Cached>,
    bytes: usize,
    clock: u64,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// Metadata reuse includes inode identity, nanosecond change time, size and mode on Unix.
/// Other platforms conservatively read content until an equivalent change token is available.
pub fn fingerprint(metadata: &Metadata) -> Option<Fingerprint> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        // Coarse change clocks cannot distinguish a rapid overwrite with restored mtime.
        if metadata.ctime_nsec() == 0 {
            return None;
        }
        Some(Fingerprint(vec![
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            u64::from(metadata.mode()),
            metadata.mtime().cast_unsigned(),
            metadata.mtime_nsec().cast_unsigned(),
            metadata.ctime().cast_unsigned(),
            metadata.ctime_nsec().cast_unsigned(),
        ]))
    }
    #[cfg(not(unix))]
    None
}

/// Reads a file, reusing a verified content observation only while its change token matches.
/// Fresh reads at settlement/review boundaries bypass the optimization entirely.
pub fn read(path: &Path, fresh: bool) -> std::io::Result<Arc<ContentObservation>> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(std::io::Error::other("content observation requires a regular file"));
    }
    let token = fingerprint(&before);
    if !fresh
        && let Some(token) = &token
        && let Ok(mut cache) = cache().lock()
    {
        cache.clock = cache.clock.wrapping_add(1);
        let clock = cache.clock;
        if let Some(entry) = cache.entries.get_mut(path)
            && &entry.fingerprint == token
        {
            entry.used = clock;
            return Ok(Arc::clone(&entry.content));
        }
    }
    let mut file = fs::File::open(path)?;
    let opened = fingerprint(&file.metadata()?);
    let mut hasher = Sha256::new();
    let mut preview = Vec::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes = bytes.saturating_add(count as u64);
        preview
            .extend_from_slice(&buffer[..count.min(PREVIEW_BYTES.saturating_sub(preview.len()))]);
    }
    if preview.contains(&0) || std::str::from_utf8(&preview).is_err() {
        preview = b"[binary content]".to_vec();
    }
    let content = Arc::new(ContentObservation { digest: hasher.finalize().into(), bytes, preview });
    // Never publish a reusable observation of a file replaced or modified during the read.
    if let Some(token) = token
        && opened.as_ref() == Some(&token)
        && fingerprint(&file.metadata()?).as_ref() == Some(&token)
        && fingerprint(&fs::symlink_metadata(path)?).as_ref() == Some(&token)
        && let Ok(mut cache) = cache().lock()
    {
        if let Some(old) = cache.entries.remove(path) {
            cache.bytes = cache.bytes.saturating_sub(old.content.preview.len() + 512);
        }
        let size = content.preview.len() + 512;
        while cache.bytes.saturating_add(size) > CACHE_BYTES {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else { break };
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes = cache.bytes.saturating_sub(old.content.preview.len() + 512);
            }
        }
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        cache.entries.insert(
            path.to_owned(),
            Cached { fingerprint: token, content: Arc::clone(&content), used },
        );
        cache.bytes = cache.bytes.saturating_add(size);
    }
    Ok(content)
}
