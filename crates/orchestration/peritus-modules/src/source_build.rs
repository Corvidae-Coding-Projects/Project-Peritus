//! Durable, explicit source-build lifecycle for market modules.

mod ownership;
mod package;
mod process;
mod source;
mod wait;

use crate::{Error, Registry, digest, invalid, market::source::CatalogSet, publish};
use ownership::{
    ProcessOwner, active_key, cancel_path, claim_owner, current_owner, owner_holds_lease,
    owner_is_live, release_owner, remove_cancel_request,
};
use peritus_plugin_sdk::LocalPackageIdentity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const RECORD_SCHEMA: u16 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BuildPhase {
    Queued,
    PreparingSource,
    PreparingDependencies,
    Compiling,
    Packaging,
    Installed,
    Failed,
    Cancelled,
    Interrupted,
}

impl BuildPhase {
    const fn terminal(self) -> bool {
        matches!(self, Self::Installed | Self::Failed | Self::Cancelled | Self::Interrupted)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildRecord {
    schema: u16,
    job_id: String,
    module_id: String,
    catalog_commit: String,
    source_digest: String,
    attempt: u32,
    phase: BuildPhase,
    detail: String,
    started_at_unix: u64,
    updated_at_unix: u64,
    active_owner: Option<ProcessOwner>,
    unit: Option<String>,
    source_archive_digest: Option<String>,
    identity: Option<LocalPackageIdentity>,
    log: String,
}

struct ActiveBuild {
    cancelled: Arc<AtomicBool>,
}

fn active() -> &'static Mutex<BTreeMap<String, ActiveBuild>> {
    static ACTIVE: OnceLock<Mutex<BTreeMap<String, ActiveBuild>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn state_lock() -> &'static Mutex<()> {
    static STATE: OnceLock<Mutex<()>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(()))
}

impl Registry {
    /// Starts or explicitly retries the selected pinned source build. The resulting package is
    /// installed disabled; this call returns after durable ownership exists, not after compilation.
    ///
    /// # Errors
    ///
    /// Returns an error when the cached source identity is absent or changed, durable ownership
    /// cannot be established, or the initial record cannot be published.
    pub fn start_source_build(
        &self,
        id: &str,
        expected_source_digest: &str,
    ) -> Result<Value, Error> {
        let set = CatalogSet::load(self.root())?;
        let entry = set
            .source
            .as_ref()
            .and_then(|catalog| catalog.modules.iter().find(|entry| entry.id == id))
            .cloned()
            .ok_or_else(|| {
                invalid("module is absent from the cached source catalog; refresh and select again")
            })?;
        if entry.source_digest != expected_source_digest {
            return Err(invalid(
                "source selection changed; refresh the menu and select the intended revision",
            ));
        }
        let key = active_key(self.root(), id);
        let _state = state_lock().lock().map_err(|_| invalid("source-build state is poisoned"))?;
        fs::create_dir_all(record_dir(self.root(), id))?;
        if let Some(mut existing) = load_record(self.root(), id)? {
            if !existing.phase.terminal()
                && active()
                    .lock()
                    .map_err(|_| invalid("source-build ownership is poisoned"))?
                    .contains_key(&key)
            {
                return Ok(json!({"source_build":existing,"already_running":true}));
            }
            if !existing.phase.terminal() {
                existing = reconcile_record(self.root(), existing)?;
                if !existing.phase.terminal() {
                    return Ok(json!({"source_build":existing,"already_running":true}));
                }
            }
        }
        let owner = current_owner()?;
        if !claim_owner(self.root(), id, &owner)? {
            return Ok(
                json!({"source_build":load_record(self.root(), id)?,"already_running":true}),
            );
        }
        remove_cancel_request(self.root(), id)?;
        let attempt =
            load_record(self.root(), id)?.map_or(1, |record| record.attempt.saturating_add(1));
        let job_id = digest(
            format!(
                "peritus.source-build.v1\0{}\0{}\0{}\0{attempt}",
                set.commit, entry.id, entry.source_digest
            )
            .as_bytes(),
        );
        let now = now()?;
        let record = BuildRecord {
            schema: RECORD_SCHEMA,
            job_id: job_id.clone(),
            module_id: entry.id.clone(),
            catalog_commit: set.commit.clone(),
            source_digest: entry.source_digest.clone(),
            attempt,
            phase: BuildPhase::Queued,
            detail: "accepted; source preparation has not started".into(),
            started_at_unix: now,
            updated_at_unix: now,
            active_owner: Some(owner.clone()),
            unit: None,
            source_archive_digest: None,
            identity: None,
            log: log_path(self.root(), id).display().to_string(),
        };
        if let Err(error) = save_record(self.root(), &record) {
            release_owner(self.root(), id, &owner);
            return Err(error);
        }
        launch_build(self, set, entry, &owner, key, &job_id)?;
        Ok(json!({"source_build":load_record(self.root(), id)?}))
    }

    /// Requests cancellation of the exact active build. A completed package is never removed.
    ///
    /// # Errors
    ///
    /// Returns an error when no live build owns the module or the cancellation marker cannot be
    /// published.
    pub fn cancel_source_build(&self, id: &str) -> Result<Value, Error> {
        let key = active_key(self.root(), id);
        let local_owner = {
            let owners =
                active().lock().map_err(|_| invalid("source-build ownership is poisoned"))?;
            owners.get(&key).map(|owner| Arc::clone(&owner.cancelled))
        };
        if let Some(cancelled) = local_owner {
            cancelled.store(true, Ordering::Release);
        } else {
            let record = load_record(self.root(), id)?
                .ok_or_else(|| invalid("this module has no active source build"))?;
            if record.phase.terminal() {
                return Err(invalid("this module has no active source build"));
            }
            let owner = record
                .active_owner
                .as_ref()
                .ok_or_else(|| invalid("this module has no active source-build owner"))?;
            if !owner_holds_lease(self.root(), id, owner) || !owner_is_live(owner)? {
                return Err(invalid("this module has no active source build"));
            }
        }
        publish(&cancel_path(self.root(), id), b"cancel\n")?;
        Ok(json!({"module":id,"cancellation_requested":true}))
    }

    /// Blocks a scriptable caller until the current build reaches a durable terminal state.
    ///
    /// # Errors
    ///
    /// Returns an error when the retained build record is absent or cannot be read.
    pub fn wait_source_build(&self, id: &str) -> Result<Value, Error> {
        wait::terminal(self, id)
    }

    /// Returns the exact retained source-build record for one installed module.
    ///
    /// # Errors
    ///
    /// Returns an error when the module has no retained build record or durable state is invalid.
    pub fn source_build_status(&self, id: &str) -> Result<Value, Error> {
        let record = self
            .source_build_records()?
            .into_iter()
            .find(|record| record["module_id"] == id)
            .ok_or_else(|| invalid("this module has no retained source-build record"))?;
        Ok(record)
    }

    pub(crate) fn source_build_records(&self) -> Result<Vec<Value>, Error> {
        let root = self.root().join("source-builds");
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else { continue };
            if let Some(mut record) = load_record(self.root(), &id)? {
                if !record.phase.terminal()
                    && !active()
                        .lock()
                        .map_err(|_| invalid("source-build ownership is poisoned"))?
                        .contains_key(&active_key(self.root(), &id))
                {
                    record = reconcile_record(self.root(), record)?;
                }
                records.push(serde_json::to_value(record)?);
            }
        }
        records.sort_by(|left, right| left["module_id"].as_str().cmp(&right["module_id"].as_str()));
        Ok(records)
    }
}

fn launch_build(
    registry: &Registry,
    set: CatalogSet,
    entry: crate::SourceEntry,
    owner: &ProcessOwner,
    key: String,
    job_id: &str,
) -> Result<(), Error> {
    let cancellation = Arc::new(AtomicBool::new(false));
    let Ok(mut owners) = active().lock() else {
        release_owner(registry.root(), &entry.id, owner);
        return Err(invalid("source-build ownership is poisoned"));
    };
    owners.insert(key.clone(), ActiveBuild { cancelled: Arc::clone(&cancellation) });
    drop(owners);

    let finished = Arc::new(AtomicBool::new(false));
    let watcher_finished = Arc::clone(&finished);
    let watcher_cancelled = Arc::clone(&cancellation);
    let watcher_root = registry.root().to_path_buf();
    let watcher_id = entry.id.clone();
    let watcher = match thread::Builder::new()
        .name(format!("peritus-source-build-cancel-{}", &job_id[..12]))
        .spawn(move || {
            while !watcher_finished.load(Ordering::Acquire) {
                if cancel_path(&watcher_root, &watcher_id).is_file() {
                    watcher_cancelled.store(true, Ordering::Release);
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }) {
        Ok(watcher) => watcher,
        Err(error) => {
            active()
                .lock()
                .map_err(|_| invalid("source-build ownership is poisoned"))?
                .remove(&key);
            update(
                registry,
                &entry.id,
                BuildPhase::Failed,
                format!("source-build cancellation watcher could not start: {error}"),
                None,
                None,
            )?;
            release_owner(registry.root(), &entry.id, owner);
            return Ok(());
        }
    };
    let owned_registry = registry.clone();
    let build_owner = owner.clone();
    let build_finished = Arc::clone(&finished);
    let id = entry.id.clone();
    let launch = thread::Builder::new()
        .name(format!("peritus-source-build-{}", &job_id[..12]))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&owned_registry, &set, &entry, &cancellation)
            }))
            .unwrap_or_else(|_| {
                Err(invalid("source-build owner panicked; partial output was not installed"))
            });
            if let Err(error) = result {
                let phase = if cancellation.load(Ordering::Acquire) {
                    BuildPhase::Cancelled
                } else {
                    BuildPhase::Failed
                };
                let _ = update(&owned_registry, &entry.id, phase, error.to_string(), None, None);
            }
            build_finished.store(true, Ordering::Release);
            let _ = watcher.join();
            remove_cancel_request(owned_registry.root(), &entry.id).ok();
            release_owner(owned_registry.root(), &entry.id, &build_owner);
            if let Ok(mut owners) = active().lock() {
                owners.remove(&key);
            }
        });
    if let Err(error) = launch {
        finished.store(true, Ordering::Release);
        active()
            .lock()
            .map_err(|_| invalid("source-build ownership is poisoned"))?
            .remove(&active_key(registry.root(), &id));
        update(
            registry,
            &id,
            BuildPhase::Failed,
            format!("source-build owner could not start: {error}"),
            None,
            None,
        )?;
        remove_cancel_request(registry.root(), &id).ok();
        release_owner(registry.root(), &id, owner);
    }
    Ok(())
}

fn run(
    registry: &Registry,
    set: &CatalogSet,
    entry: &crate::SourceEntry,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), Error> {
    update(
        registry,
        &entry.id,
        BuildPhase::PreparingSource,
        "capturing the pinned clean source revision".into(),
        None,
        None,
    )?;
    let temporary = tempfile::Builder::new()
        .prefix("source-build-")
        .tempdir_in(record_dir(registry.root(), &entry.id))?;
    let prepared = source::prepare(set, entry, temporary.path(), cancelled)?;
    update(
        registry,
        &entry.id,
        BuildPhase::PreparingDependencies,
        "preparing locked public dependencies in a private credential-free cache".into(),
        None,
        prepared.archive_digest.clone(),
    )?;
    let context =
        package::BuildContext::prepare(registry, entry, &prepared, temporary.path(), cancelled)?;
    update(
        registry,
        &entry.id,
        BuildPhase::Compiling,
        "compiling offline with network denied in the bounded Linux build scope".into(),
        None,
        None,
    )?;
    let executable = context.compile(registry, entry, cancelled)?;
    update(
        registry,
        &entry.id,
        BuildPhase::Packaging,
        "validating executable and package identities before atomic disabled installation".into(),
        None,
        None,
    )?;
    let identity = package::install(registry, set, entry, &prepared, &context, &executable)?;
    update(
        registry,
        &entry.id,
        BuildPhase::Installed,
        "source build installed outside Git and remains disabled until explicitly enabled".into(),
        Some(identity),
        None,
    )?;
    Ok(())
}

pub fn set_unit(registry: &Registry, id: &str, unit: Option<String>) -> Result<(), Error> {
    let _state = state_lock().lock().map_err(|_| invalid("source-build state is poisoned"))?;
    let mut record = load_record(registry.root(), id)?
        .ok_or_else(|| invalid("source-build record disappeared"))?;
    record.unit = unit;
    record.updated_at_unix = now()?;
    save_record(registry.root(), &record)
}

fn update(
    registry: &Registry,
    id: &str,
    phase: BuildPhase,
    detail: String,
    identity: Option<LocalPackageIdentity>,
    archive: Option<String>,
) -> Result<(), Error> {
    let _state = state_lock().lock().map_err(|_| invalid("source-build state is poisoned"))?;
    let mut record = load_record(registry.root(), id)?
        .ok_or_else(|| invalid("source-build record disappeared"))?;
    record.phase = phase;
    record.detail = detail;
    record.updated_at_unix = now()?;
    record.unit = None;
    if phase.terminal() {
        record.active_owner = None;
    }
    if identity.is_some() {
        record.identity = identity;
    }
    if archive.is_some() {
        record.source_archive_digest = archive;
    }
    save_record(registry.root(), &record)
}

fn reconcile_record(root: &Path, mut record: BuildRecord) -> Result<BuildRecord, Error> {
    if let Some(owner) = &record.active_owner
        && owner_holds_lease(root, &record.module_id, owner)
        && owner_is_live(owner)?
    {
        return Ok(record);
    }
    let prior_owner = record.active_owner.take();
    if let Some(unit) = record.unit.take() {
        process::terminate_unit(&unit);
    }
    record.phase = BuildPhase::Interrupted;
    record.detail =
        "host ownership ended before a terminal build receipt; partial output was not installed"
            .into();
    record.updated_at_unix = now()?;
    save_record(root, &record)?;
    if let Some(owner) = prior_owner {
        release_owner(root, &record.module_id, &owner);
    }
    remove_cancel_request(root, &record.module_id).ok();
    Ok(record)
}

fn record_dir(root: &Path, id: &str) -> PathBuf {
    root.join("source-builds").join(id)
}

fn record_path(root: &Path, id: &str) -> PathBuf {
    record_dir(root, id).join("record.json")
}

fn log_path(root: &Path, id: &str) -> PathBuf {
    record_dir(root, id).join("build.log")
}

pub fn build_log(registry: &Registry, id: &str) -> PathBuf {
    log_path(registry.root(), id)
}

pub fn build_job_id(registry: &Registry, id: &str) -> Result<String, Error> {
    load_record(registry.root(), id)?
        .map(|record| record.job_id)
        .ok_or_else(|| invalid("source-build record disappeared"))
}

fn load_record(root: &Path, id: &str) -> Result<Option<BuildRecord>, Error> {
    match fs::read(record_path(root, id)) {
        Ok(bytes) => {
            let record: BuildRecord = serde_json::from_slice(&bytes)?;
            if record.schema != RECORD_SCHEMA || record.module_id != id {
                return Err(invalid("source-build record identity is invalid"));
            }
            Ok(Some(record))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn save_record(root: &Path, record: &BuildRecord) -> Result<(), Error> {
    publish(&record_path(root, &record.module_id), &serde_json::to_vec_pretty(record)?)
}

fn now() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| invalid(error.to_string()))
}
