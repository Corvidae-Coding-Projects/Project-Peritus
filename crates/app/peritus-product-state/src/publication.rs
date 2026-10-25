//! Per-record mutation ownership and short-lock publication of immutable read snapshots.
//!
//! A writer may perform durable I/O while readers keep the previously published state. Writers
//! of distinct keys do not serialize behind that I/O.
//!
//! Callers publish only after their own durability/admission checks; this container grants no
//! execution authority.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
    sync::{Arc, Condvar, Mutex, OnceLock, RwLock, Weak},
};

#[derive(Default)]
struct Gate {
    busy: Mutex<bool>,
    ready: Condvar,
}

/// An owned serialization permit; releasing it wakes the next writer for this exact key.
pub struct PublicationPermit {
    gate: Arc<Gate>,
}

impl Drop for PublicationPermit {
    fn drop(&mut self) {
        *self.gate.busy.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        self.gate.ready.notify_one();
    }
}

/// Exact-key admission gates. Idle entries are weak and do not retain historical records.
pub struct PublicationGates<K> {
    entries: Mutex<BTreeMap<K, Weak<Gate>>>,
}

impl<K> Default for PublicationGates<K> {
    fn default() -> Self {
        Self { entries: Mutex::new(BTreeMap::new()) }
    }
}

impl<K: Ord + Clone> PublicationGates<K> {
    /// Waits for exclusive ownership of this key without holding a registry or another key.
    #[must_use]
    pub fn enter(&self, key: K) -> PublicationPermit {
        let gate = {
            let mut entries =
                self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            entries.retain(|_, gate| gate.strong_count() > 0);
            entries.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
                let gate = Arc::new(Gate::default());
                entries.insert(key, Arc::downgrade(&gate));
                gate
            })
        };
        let mut busy = gate.busy.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        while *busy {
            busy = gate.ready.wait(busy).unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *busy = true;
        drop(busy);
        PublicationPermit { gate }
    }
}

type Loader<V> = dyn Fn() -> Option<V> + Send + Sync;

struct Entry<V> {
    hint: Arc<V>,
    value: OnceLock<Arc<V>>,
    loader: Option<Box<Loader<V>>>,
    loading: Mutex<()>,
}
impl<V> Entry<V> {
    fn loaded(value: Arc<V>) -> Self {
        Self {
            hint: Arc::clone(&value),
            value: OnceLock::from(value),
            loader: None,
            loading: Mutex::new(()),
        }
    }
    fn get(&self) -> Option<&Arc<V>> {
        if let Some(value) = self.value.get() {
            return Some(value);
        }
        let _loading = self.loading.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.value.get().is_none() {
            // Failures are not cached: a transient storage failure may be repaired and retried.
            let value = self.loader.as_ref()?()?;
            let _ = self.value.set(Arc::new(value));
        }
        self.value.get()
    }
}

/// Registry with short-lock publication and optional demand-loaded historical values.
pub struct PublishedRecords<K, V> {
    records: RwLock<Arc<BTreeMap<K, Arc<Entry<V>>>>>,
    writers: PublicationGates<K>,
}

/// Stable pointers to published records. Hints never grant authority to execute or mutate.
pub struct PublishedView<K, V>(Arc<BTreeMap<K, Arc<Entry<V>>>>);
impl<K: Ord, V> PublishedView<K, V> {
    /// Loads only this exact record, returning no record if its loader rejects durable state.
    pub fn get(&self, key: &K) -> Option<&Arc<V>> {
        self.0.get(key)?.get()
    }
    /// Reports index membership without loading historical payloads.
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }
    /// Iterates identifiers without loading payloads.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.0.keys()
    }
    /// Iterates successfully loaded values. Prefer hints for metadata-only filtering.
    pub fn values(&self) -> impl Iterator<Item = &Arc<V>> {
        self.0.values().filter_map(|entry| entry.get())
    }
    /// Iterates successfully loaded records and identifiers.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &Arc<V>)> {
        self.0.iter().filter_map(|(key, entry)| Some((key, entry.get()?)))
    }
    /// Returns cheap metadata for selection only. Use `get` before returning or acting on a run.
    pub fn hints(&self) -> impl Iterator<Item = (&K, &Arc<V>)> {
        self.0.iter().map(|(key, entry)| (key, entry.value.get().unwrap_or(&entry.hint)))
    }
    /// Number of indexed records, including unloaded records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether this view has no indexed records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Per-key pending changes. Ordinary scope exit publishes changed keys after durable work.
/// Unwinding discards unpublished changes. The caller's previously published state stays visible.
pub struct PendingRecords<'a, K: Ord + Clone, V: Clone> {
    owner: &'a PublishedRecords<K, V>,
    local: PublishedView<K, V>,
    permits: BTreeMap<K, PublicationPermit>,
    dirty: BTreeSet<K>,
}

impl<K: Ord + Clone, V: Clone> PublishedRecords<K, V> {
    /// Creates a registry from already validated records.
    #[must_use]
    pub fn new(records: BTreeMap<K, V>) -> Self {
        Self {
            records: RwLock::new(Arc::new(
                records
                    .into_iter()
                    .map(|(key, value)| (key, Arc::new(Entry::loaded(Arc::new(value)))))
                    .collect(),
            )),
            writers: PublicationGates::default(),
        }
    }

    /// Registers indexed history without opening its payload. The loader must validate full
    /// authoritative state before returning a value; the hint is never returned by `get`.
    ///
    /// # Errors
    ///
    /// Returns an error if the publication map is poisoned.
    pub fn insert_deferred(
        &self,
        key: K,
        hint: V,
        loader: impl Fn() -> Option<V> + Send + Sync + 'static,
    ) -> Result<(), PublicationError> {
        let _permit = self.writers.enter(key.clone());
        let mut records = self.records.write().map_err(|_| PublicationError)?;
        Arc::make_mut(&mut records).entry(key).or_insert_with(|| {
            Arc::new(Entry {
                hint: Arc::new(hint),
                value: OnceLock::new(),
                loader: Some(Box::new(loader)),
                loading: Mutex::new(()),
            })
        });
        Ok(())
    }

    /// Takes an O(1) shared map snapshot, without copying historical keys or record payloads.
    ///
    /// # Errors
    /// Returns an error if the publication map was poisoned.
    pub fn read(&self) -> Result<PublishedView<K, V>, PublicationError> {
        self.records
            .read()
            .map(|records| PublishedView(Arc::clone(&records)))
            .map_err(|_| PublicationError)
    }

    /// Same non-waiting-for-writers observation as `read`.
    ///
    /// # Errors
    /// Returns an error if the short publication operation is busy or poisoned.
    pub fn try_read(&self) -> Result<PublishedView<K, V>, PublicationError> {
        self.records
            .try_read()
            .map(|records| PublishedView(Arc::clone(&records)))
            .map_err(|_| PublicationError)
    }

    /// Begins a pending view. Exclusive ownership is acquired lazily for each mutated key.
    ///
    /// # Errors
    /// Returns an error if the publication map was poisoned.
    pub fn write(&self) -> Result<PendingRecords<'_, K, V>, PublicationError> {
        Ok(PendingRecords {
            owner: self,
            local: self.read()?,
            permits: BTreeMap::new(),
            dirty: BTreeSet::new(),
        })
    }
}

impl<K: Ord + Clone, V: Clone> Deref for PendingRecords<'_, K, V> {
    type Target = PublishedView<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

impl<K: Ord + Clone, V: Clone> PendingRecords<'_, K, V> {
    fn acquire(&mut self, key: &K) {
        if !self.permits.contains_key(key) {
            let permit = self.owner.writers.enter(key.clone());
            // Another same-key writer may have committed while this writer waited.
            let current = self
                .owner
                .records
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(key)
                .cloned();
            if let Some(current) = current {
                Arc::make_mut(&mut self.local.0).insert(key.clone(), current);
            } else {
                Arc::make_mut(&mut self.local.0).remove(key);
            }
            self.permits.insert(key.clone(), permit);
        }
        self.dirty.insert(key.clone());
    }

    /// Borrows this key's private working copy, refreshed after the preceding writer commits.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.acquire(key);
        let value = Arc::clone(self.local.get(key)?);
        Arc::make_mut(&mut self.local.0).insert(key.clone(), Arc::new(Entry::loaded(value)));
        let entry = Arc::get_mut(Arc::make_mut(&mut self.local.0).get_mut(key)?)?;
        entry.hint = Arc::clone(entry.value.get()?);
        entry.value.get_mut().map(Arc::make_mut)
    }

    /// Stages a new or replacement record under its exact-key permit.
    pub fn insert(&mut self, key: K, value: V) -> Option<Arc<V>> {
        self.acquire(&key);
        Arc::make_mut(&mut self.local.0)
            .insert(key, Arc::new(Entry::loaded(Arc::new(value))))
            .and_then(|entry| entry.get().cloned())
    }

    /// Stages removal; published readers retain their existing immutable views.
    pub fn remove(&mut self, key: &K) -> Option<Arc<V>> {
        self.acquire(key);
        Arc::make_mut(&mut self.local.0).remove(key).and_then(|entry| entry.get().cloned())
    }
}

impl<K: Ord + Clone, V: Clone> Drop for PendingRecords<'_, K, V> {
    fn drop(&mut self) {
        if std::thread::panicking() || self.dirty.is_empty() {
            return;
        }
        let mut records =
            self.owner.records.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        {
            let published = Arc::make_mut(&mut records);
            for key in &self.dirty {
                if let Some(value) = self.local.0.get(key) {
                    published.insert(key.clone(), Arc::clone(value));
                } else {
                    published.remove(key);
                }
            }
        }
        drop(records);
    }
}

/// The short shared publication map could not be observed.
#[derive(Clone, Copy, Debug)]
pub struct PublicationError;
impl std::fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("published record registry is unavailable")
    }
}
impl std::error::Error for PublicationError {}
