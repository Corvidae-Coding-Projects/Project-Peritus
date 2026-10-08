//! Durable console discovery, launch, adoption, and owner reaping.

use super::{
    Terminal, identity, owner_live, reconcile,
    record::{
        CloseDisposition, ConsoleRecord, LAUNCH_FILE, LaunchManifest, RECORD_FILE, directory,
        valid_identity,
    },
};
use crate::{
    consoles::Console,
    error::{Result, problem, uncertain},
    state::{id, save},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const OWNER_LOG_FILE: &str = "owner.log";
const LEGACY_ASSOCIATIONS: &str = "legacy";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyAssociation {
    schema_version: u32,
    workspace: String,
    store: String,
    state_file: PathBuf,
    operation: String,
    console: Console,
    source: PathBuf,
}

pub(crate) struct TerminalRegistry {
    state_file: PathBuf,
    workspace: String,
    store: String,
    root: PathBuf,
    archive: PathBuf,
    legacy_active: PathBuf,
    legacy_archive: PathBuf,
    legacy_associations: PathBuf,
    terminals: Mutex<BTreeMap<String, Arc<Terminal>>>,
    reapers: Mutex<Vec<thread::JoinHandle<()>>>,
    refreshing: Mutex<()>,
}

impl TerminalRegistry {
    pub(crate) fn open(state_file: &Path, workspace: &str) -> Result<Self> {
        let state_file = state_file.canonicalize()?;
        if !valid_identity(workspace) {
            return Err(problem("Workspace identity is invalid for console storage"));
        }
        let parent = state_file.parent().ok_or_else(|| problem("State path has no parent"))?;
        let mut digest = Sha256::new();
        digest.update(b"peritus-console-store-v2\0");
        digest.update(state_file.as_os_str().as_encoded_bytes());
        digest.update(b"\0");
        digest.update(workspace.as_bytes());
        let store = crate::state::hex(&digest.finalize());
        let legacy = parent.join("consoles");
        let consoles = legacy.join("stores").join(&store);
        let root = consoles.join("active");
        let archive = consoles.join("archive");
        let legacy_associations = consoles.join(LEGACY_ASSOCIATIONS);
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(&archive)?;
        std::fs::create_dir_all(&legacy_associations)?;
        restrict_directory(&root)?;
        restrict_directory(&archive)?;
        restrict_directory(&legacy_associations)?;
        let value = Self {
            state_file,
            workspace: workspace.to_owned(),
            store,
            root,
            archive,
            legacy_active: legacy.join("active"),
            legacy_archive: legacy.join("archive"),
            legacy_associations,
            terminals: Mutex::new(BTreeMap::new()),
            reapers: Mutex::new(Vec::new()),
            refreshing: Mutex::new(()),
        };
        Ok(value)
    }

    pub(crate) fn refresh(&self) -> Result<()> {
        let _refreshing = self.refreshing.lock().map_err(problem)?;
        self.reap_finished()?;
        let mut discovered = BTreeMap::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(identity) = entry.file_name().to_str().map(str::to_owned) else { continue };
            let Ok(path) = directory(&self.root, &identity) else { continue };
            let record_path = path.join(RECORD_FILE);
            if !record_path.is_file() {
                continue;
            }
            let record = reconcile(&record_path)?;
            self.validate_namespace(&record)?;
            if record.closed() {
                if record.ended() && !owner_live(&record) {
                    archive(&path, &self.archive.join(&identity))?;
                }
            } else {
                if discovered
                    .insert(identity, Arc::new(Terminal {
                        directory: path,
                        workspace: self.workspace.clone(),
                    }))
                    .is_some()
                {
                    return Err(problem("Console identity is duplicated in its state namespace"));
                }
            }
        }
        for entry in std::fs::read_dir(&self.legacy_associations)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let association = self.read_legacy_association(&entry.path())?;
            let record = reconcile(&association.source.join(RECORD_FILE))?;
            if !record.legacy()
                || record.operation != association.operation
                || record.console != association.console
            {
                return Err(problem(
                    "A retained legacy console conflicts with its exact migration receipt",
                ));
            }
            if !record.closed() {
                if discovered
                    .insert(
                        record.console.id.clone(),
                        Arc::new(Terminal {
                            directory: association.source,
                            workspace: self.workspace.clone(),
                        }),
                    )
                    .is_some()
                {
                    return Err(problem(
                        "Legacy console identity collides with this state namespace",
                    ));
                }
            }
        }
        let mut terminals = self.terminals.lock().map_err(problem)?;
        *terminals = discovered;
        Ok(())
    }

    pub(crate) fn get(&self, identity: &str) -> Result<Arc<Terminal>> {
        self.refresh()?;
        self.terminals
            .lock()
            .map_err(problem)?
            .get(identity)
            .cloned()
            .ok_or_else(|| problem("Console is closed or unavailable"))
    }

    pub(crate) fn get_for_workspace(
        &self,
        workspace: &str,
        identity: &str,
    ) -> Result<Arc<Terminal>> {
        if workspace != self.workspace {
            return Err(problem("Console belongs to another gateway workspace"));
        }
        self.get(identity)
    }

    pub(crate) fn list(&self) -> Result<Vec<Arc<Terminal>>> {
        self.refresh()?;
        Ok(self.terminals.lock().map_err(problem)?.values().cloned().collect())
    }

    pub(crate) fn launch(
        &self,
        operation: &str,
        console: Console,
        executable: &Path,
        working_directory: &Path,
        arguments: Vec<String>,
    ) -> Result<()> {
        let path = directory(&self.root, &console.id)?;
        std::fs::create_dir(&path)?;
        restrict_directory(&path)?;
        let token = id()?;
        let manifest_path = path.join(LAUNCH_FILE);
        LaunchManifest::new(
            operation.to_owned(),
            console.clone(),
            token,
            self.workspace.clone(),
            self.store.clone(),
            self.state_file.clone(),
            executable.to_owned(),
            working_directory.to_owned(),
            arguments,
        )
        .publish(&manifest_path)?;

        let log = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path.join(OWNER_LOG_FILE))?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--console-owner")
            .arg(&manifest_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        isolate_owner(&mut command);
        let mut owner = command.spawn()?;
        let record_path = path.join(RECORD_FILE);
        let record = loop {
            match ConsoleRecord::read(&record_path) {
                Ok(record) => break record,
                Err(_) if !record_path.exists() => {}
                Err(error) => return Err(error),
            }
            if let Some(status) = owner.try_wait()? {
                return Err(problem(format!(
                    "Console owner exited during launch ({status}); inspect {}",
                    path.join(OWNER_LOG_FILE).display()
                )));
            }
            thread::sleep(Duration::from_millis(10));
        };
        if record.operation != operation
            || record.console != console
            || record.workspace != self.workspace
            || record.store != self.store
            || record.state_file != self.state_file
            || record.owner.pid != owner.id()
            || identity::current_start_token(record.owner.pid) != Some(record.owner.start_token)
        {
            return Err(uncertain(
                "Console owner publication conflicts with the accepted launch",
            ));
        }
        self.terminals
            .lock()
            .map_err(uncertain)?
            .insert(console.id, Arc::new(Terminal {
                directory: path,
                workspace: self.workspace.clone(),
            }));
        let reaper = thread::Builder::new()
            .name("peritus-console-owner-reaper".into())
            .spawn(move || {
                let _ = owner.wait();
            })
            .map_err(uncertain)?;
        self.reapers.lock().map_err(uncertain)?.push(reaper);
        Ok(())
    }

    pub(crate) fn recover_launch(
        &self,
        operation: &str,
        expected: &Console,
    ) -> Result<Option<Value>> {
        let record = match self.record(&expected.id)? {
            Some(record) => record,
            None => match self.unassociated_legacy_record(&expected.id)? {
                Some((record, source)) => {
                    if record.operation != operation || record.console != *expected {
                        return Err(problem(
                            "The legacy console conflicts with its exact launch receipt",
                        ));
                    }
                    self.associate_legacy(operation, expected, &source)?;
                    self.terminals.lock().map_err(problem)?.insert(
                        expected.id.clone(),
                        Arc::new(Terminal {
                            directory: source,
                            workspace: self.workspace.clone(),
                        }),
                    );
                    record
                }
                None => return Ok(None),
            },
        };
        if record.operation != operation || record.console != *expected {
            return Err(problem(
                "The durable console launch conflicts with its original operation receipt",
            ));
        }
        if !record.legacy() {
            self.validate_namespace(&record)?;
        }
        Ok(Some(json!(record.console)))
    }

    pub(crate) fn recover_close(
        &self,
        identity: &str,
        disposition: CloseDisposition,
    ) -> Result<Option<Value>> {
        let Some(record) = self.record(identity)? else {
            return Ok(None);
        };
        if !record.close_confirmed {
            return Ok(None);
        }
        if record.close_disposition != Some(disposition) {
            return Err(problem(
                "The durable console close conflicts with its original operation receipt",
            ));
        }
        let disposition = match disposition {
            CloseDisposition::Terminate => "terminate",
            CloseDisposition::Dismiss => "dismiss",
        };
        Ok(Some(json!({"closed":true,"disposition":disposition,"id":identity})))
    }

    pub(crate) fn remove(&self, identity: &str) -> Result<()> {
        self.terminals.lock().map_err(problem)?.remove(identity);
        Ok(())
    }

    fn reap_finished(&self) -> Result<()> {
        let mut reapers = self.reapers.lock().map_err(problem)?;
        let mut index = 0;
        while index < reapers.len() {
            if reapers[index].is_finished() {
                let _ = reapers.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn record(&self, identity: &str) -> Result<Option<ConsoleRecord>> {
        let _refreshing = self.refreshing.lock().map_err(problem)?;
        let active = directory(&self.root, identity)?.join(RECORD_FILE);
        let archived = directory(&self.archive, identity)?.join(RECORD_FILE);
        match (active.is_file(), archived.is_file()) {
            (true, true) => Err(problem(
                "The console identity exists in both active and archived storage",
            )),
            (true, false) => ConsoleRecord::read(&active).and_then(|record| {
                self.validate_namespace(&record)?;
                Ok(Some(record))
            }),
            (false, true) => ConsoleRecord::read(&archived).and_then(|record| {
                self.validate_namespace(&record)?;
                Ok(Some(record))
            }),
            (false, false) => {
                let association_path = self.association_path(identity)?;
                if !association_path.is_file() {
                    return Ok(None);
                }
                let association = self.read_legacy_association(&association_path)?;
                let record = ConsoleRecord::read(&association.source.join(RECORD_FILE))?;
                if !record.legacy()
                    || record.operation != association.operation
                    || record.console != association.console
                {
                    return Err(problem(
                        "A retained legacy console conflicts with its migration receipt",
                    ));
                }
                Ok(Some(record))
            }
        }
    }

    fn association_path(&self, identity: &str) -> Result<PathBuf> {
        let _ = directory(&self.root, identity)?;
        Ok(self.legacy_associations.join(format!("{identity}.json")))
    }

    fn associate_legacy(&self, operation: &str, console: &Console, source: &Path) -> Result<()> {
        let value = LegacyAssociation {
            schema_version: 1,
            workspace: self.workspace.clone(),
            store: self.store.clone(),
            state_file: self.state_file.clone(),
            operation: operation.to_owned(),
            console: console.clone(),
            source: source.to_owned(),
        };
        save(
            &self.association_path(&console.id)?,
            &serde_json::to_vec(&value)?,
        )
    }

    fn read_legacy_association(&self, path: &Path) -> Result<LegacyAssociation> {
        let value: LegacyAssociation = serde_json::from_slice(&std::fs::read(path)?)?;
        let expected_source_active = directory(&self.legacy_active, &value.console.id)?;
        let expected_source_archive = directory(&self.legacy_archive, &value.console.id)?;
        if value.schema_version != 1
            || value.workspace != self.workspace
            || value.store != self.store
            || value.state_file != self.state_file
            || path != self.association_path(&value.console.id)?
            || value.source != expected_source_active && value.source != expected_source_archive
        {
            return Err(problem("Legacy console migration receipt is invalid"));
        }
        Ok(value)
    }

    fn unassociated_legacy_record(
        &self,
        identity: &str,
    ) -> Result<Option<(ConsoleRecord, PathBuf)>> {
        let active = directory(&self.legacy_active, identity)?;
        let archived = directory(&self.legacy_archive, identity)?;
        let (source, record) = match (
            active.join(RECORD_FILE).is_file(),
            archived.join(RECORD_FILE).is_file(),
        ) {
            (true, true) => return Err(problem("Legacy console identity is ambiguous")),
            (true, false) => {
                let record = ConsoleRecord::read(&active.join(RECORD_FILE))?;
                (active, record)
            }
            (false, true) => {
                let record = ConsoleRecord::read(&archived.join(RECORD_FILE))?;
                (archived, record)
            }
            (false, false) => return Ok(None),
        };
        if !record.legacy() {
            return Err(problem("Unassociated console record is not a legacy record"));
        }
        Ok(Some((record, source)))
    }

    fn validate_namespace(&self, record: &ConsoleRecord) -> Result<()> {
        if record.workspace != self.workspace
            || record.store != self.store
            || record.state_file != self.state_file
        {
            return Err(problem("Console record belongs to another state namespace"));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn isolate_owner(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[cfg(windows)]
fn isolate_owner(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    command.creation_flags(
        CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB,
    );
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn archive(active: &Path, archived: &Path) -> Result<()> {
    if archived.exists() {
        if !active.exists() && archived.is_dir() {
            return Ok(());
        }
        return Err(problem("Archived console identity already exists"));
    }
    match std::fs::rename(active, archived) {
        Ok(()) => {}
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && !active.exists()
                && archived.is_dir() =>
        {
            return Ok(())
        }
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        std::fs::File::open(
            active.parent().ok_or_else(|| problem("Active console path has no parent"))?,
        )?
        .sync_all()?;
        std::fs::File::open(
            archived.parent().ok_or_else(|| problem("Console archive path has no parent"))?,
        )?
        .sync_all()?;
    }
    Ok(())
}

#[cfg(windows)]
fn restrict_directory(path: &Path) -> Result<()> {
    let _ = std::fs::metadata(path)?;
    Ok(())
}
