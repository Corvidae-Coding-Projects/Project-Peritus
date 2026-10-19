//! Bounded decoded projections of immutable, durably owned message artifacts.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{Message, ProtocolLimits, decode_messages};
use peritus_types::Sha256Digest;

use super::super::{error, record::ArchiveKind};
use super::LocalMemory;

const CACHE_BYTES: usize = 64 * 1024 * 1024;
type Key = (Sha256Digest, Sha256Digest);

struct Entry {
    message: Arc<Message>,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: BTreeMap<Key, Entry>,
    bytes: usize,
    clock: u64,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

impl LocalMemory {
    pub(in crate::local_context) fn message(
        &self,
        sequence: u64,
    ) -> Result<Arc<Message>, DeveloperLoopError> {
        let source = self.archived(sequence)?;
        if source.kind == ArchiveKind::ToolOutput {
            return Err(error("tool observation is not an encoded message"));
        }
        let key = (self.store.scope_digest(), source.artifact.digest);
        if let Ok(mut cache) = cache().lock() {
            cache.clock = cache.clock.wrapping_add(1);
            let used = cache.clock;
            if let Some(entry) = cache.entries.get_mut(&key) {
                entry.used = used;
                self.decoded_hits.set(self.decoded_hits.get().saturating_add(1));
                return Ok(Arc::clone(&entry.message));
            }
        }
        // A cold projection still uses the store's size/digest validation. Cached typed
        // observations are owned by this lineage, like the in-memory working-state reducer;
        // they never cache file freshness, permission decisions or assembled host policy.
        let bytes = self.artifact(sequence)?;
        let mut decoded = decode_messages(&bytes, ProtocolLimits::PRODUCTION)?;
        if decoded.len() != 1 {
            return Err(error("message observation contains multiple messages"));
        }
        let message = Arc::new(decoded.pop().ok_or_else(|| error("empty message observation"))?);
        self.decoded_misses.set(self.decoded_misses.get().saturating_add(1));
        let size = bytes
            .len()
            .saturating_mul(4)
            .saturating_add(message.content().len().saturating_mul(512))
            .saturating_add(512);
        if size <= CACHE_BYTES
            && let Ok(mut cache) = cache().lock()
        {
            if let Some(old) = cache.entries.remove(&key) {
                cache.bytes = cache.bytes.saturating_sub(old.bytes);
            }
            while cache.bytes.saturating_add(size) > CACHE_BYTES {
                let oldest =
                    cache.entries.iter().min_by_key(|(_, entry)| entry.used).map(|(key, _)| *key);
                let Some(oldest) = oldest else { break };
                if let Some(old) = cache.entries.remove(&oldest) {
                    cache.bytes = cache.bytes.saturating_sub(old.bytes);
                }
            }
            cache.clock = cache.clock.wrapping_add(1);
            let used = cache.clock;
            cache.entries.insert(key, Entry { message: Arc::clone(&message), bytes: size, used });
            cache.bytes = cache.bytes.saturating_add(size);
        }
        Ok(message)
    }
}
