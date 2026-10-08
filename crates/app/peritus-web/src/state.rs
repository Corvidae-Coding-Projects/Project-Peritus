//! Durable browser workspace and original operation outcomes.

mod document;
mod operation_store;

pub(crate) use operation_store::{OperationOwner, PendingPage, Publication};

use crate::{
    config::{Options, Preferences},
    error::{Result, problem},
    terminal::TerminalRegistry,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
    pub(crate) name: String,
    pub(crate) repository: PathBuf,
    pub(crate) closed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub(crate) settings: crate::sessions::Settings,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) conversation: String,
    #[serde(default)]
    pub(crate) run: String,
    #[serde(default)]
    pub(crate) native: Option<crate::daemon::NativeOwner>,
    pub(crate) project: String,
    pub(crate) parent: Option<String>,
    pub(crate) title: String,
    pub(crate) closed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub(crate) input: Value,
    pub(crate) prepared: Option<Value>,
    pub(crate) result: Option<Value>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default)]
    pub(crate) identity: String,
    pub(crate) projects: Vec<Project>,
    pub(crate) sessions: Vec<Session>,
    pub(crate) attachments: BTreeMap<String, crate::files::attachments::Attachment>,
}
pub struct App {
    pub(crate) options: Options,
    pub(crate) port: u16,
    pub(crate) token: String,
    pub(crate) workspace: Mutex<Workspace>,
    operations: operation_store::OperationStore,
    pub(crate) terminals: Arc<TerminalRegistry>,
    locks: Mutex<BTreeMap<String, Weak<tokio::sync::Mutex<()>>>>,
}
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [char::from(DIGITS[usize::from(byte >> 4)]), char::from(DIGITS[usize::from(byte & 15)])]
        })
        .collect()
}
pub fn id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(problem)?;
    Ok(hex(&bytes))
}
pub fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| problem("No parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(problem)?;
    #[cfg(unix)]
    std::fs::File::open(parent).and_then(|directory| directory.sync_all())
        .map_err(crate::error::uncertain)?;
    Ok(())
}
impl App {
    pub(crate) fn open(options: Options, port: u16) -> Result<Self> {
        let decoded = match std::fs::read(&options.state_file) {
            Ok(bytes) => {
                let decoded = document::decode(&options.state_file, &bytes)?;
                if decoded.migration {
                    document::preserve_legacy(&options.state_file, &bytes)?;
                }
                decoded
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => document::Decoded {
                workspace: Workspace::default(),
                operations: BTreeMap::new(),
                migration: true,
            },
            Err(error) => return Err(error.into()),
        };
        let mut workspace = decoded.workspace;
        let new_identity = workspace.identity.is_empty();
        if new_identity { workspace.identity = id()?; }
        let operations = operation_store::OperationStore::open(
            &options.state_file,
            &workspace.identity,
        )?;
        operations.import(&decoded.operations)?;
        let recovered_operations = recover_unsubmitted_native_operations(&operations)?;
        if decoded.migration || new_identity || recovered_operations {
            save(&options.state_file, &document::encode(&workspace)?)?;
        }
        if !options.config_file.exists() {
            save(
                &options.config_file,
                toml::to_string_pretty(&Preferences::default()).map_err(problem)?.as_bytes(),
            )?;
        }
        let terminals = Arc::new(TerminalRegistry::open(
            &options.state_file,
            &workspace.identity,
        )?);
        let app = Self {
            options,
            port,
            token: id()?,
            workspace: Mutex::new(workspace),
            operations,
            terminals,
            locks: Mutex::new(BTreeMap::new()),
        };
        if app.snapshot()?.projects.is_empty() {
            app.open_project(&app.options.root)?;
        }
        Ok(app)
    }
    pub(crate) fn snapshot(&self) -> Result<Workspace> {
        Ok(self.workspace.lock().map_err(problem)?.clone())
    }
    pub(crate) fn lock(&self, key: String) -> Result<Arc<tokio::sync::Mutex<()>>> {
        let mut locks = self.locks.lock().map_err(problem)?;
        if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(key, Arc::downgrade(&lock));
        drop(locks);
        Ok(lock)
    }
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Workspace) -> Result<T>) -> Result<T> {
        let mut locked = self.workspace.lock().map_err(problem)?;
        let mut next = locked.clone();
        let result = change(&mut next)?;
        let published = save(&self.options.state_file, &document::encode(&next)?);
        // A directory-sync failure follows successful replacement. Retain that exact visible
        // ledger in memory while reporting uncertain durability; stale memory must not admit
        // the same original effect again after its identity was published on disk.
        if published.is_ok() || published.as_ref().is_err_and(|error| error.1) {
            *locked = next;
        }
        drop(locked);
        published?;
        Ok(result)
    }
    pub(crate) fn record_operation(&self, operation: String, input: Value) -> Result<()> {
        self.operations.owner(&operation)?.insert(input).map(|_| ())
    }
    pub(crate) fn retain_prepared_operation(&self, operation: &str, prepared: Value) -> Result<()> {
        self.operations.owner(operation)?.retain_prepared(prepared)
    }
    pub(crate) fn operation(&self, operation: &str) -> Result<Option<Operation>> {
        self.operations.get(operation)
    }
    pub(crate) async fn own_operation(&self, operation: &str) -> Result<OperationOwner> {
        let store = self.operations.clone();
        let operation = operation.to_owned();
        tokio::task::spawn_blocking(move || store.owner(&operation)).await.map_err(problem)?
    }
    pub(crate) async fn try_own_operation(&self, operation: &str) -> Result<Option<OperationOwner>> {
        let store = self.operations.clone();
        let operation = operation.to_owned();
        tokio::task::spawn_blocking(move || store.try_owner(&operation)).await.map_err(problem)?
    }
    pub(crate) fn settle_operation(&self, operation: &str, result: Value) -> Result<()> {
        self.operations.owner(operation)?.settle(result)
    }
    pub(crate) fn pending_operations(
        &self,
        after: Option<&str>,
        snapshot: Option<&str>,
    ) -> Result<PendingPage> {
        self.operations.pending_page(after, snapshot)
    }
    pub(crate) fn project(&self, id: &str) -> Result<Project> {
        self.snapshot()?
            .projects
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| problem("Project is no longer open"))
    }
    pub(crate) fn session(&self, id: &str) -> Result<Session> {
        self.snapshot()?
            .sessions
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| problem("Session not found"))
    }
    pub(crate) fn session_owner(&self, id: &str) -> Result<Option<crate::daemon::NativeOwner>> {
        let workspace = self.snapshot()?;
        let mut cursor = Some(id);
        let mut visited = std::collections::BTreeSet::new();
        let mut selected = None;
        while let Some(id) = cursor {
            if !visited.insert(id) {
                return Err(problem("Retained session ancestry contains a cycle"));
            }
            let session = workspace
                .sessions
                .iter()
                .find(|session| session.id == id)
                .ok_or_else(|| problem("Retained session ancestry references an unknown parent"))?;
            if let Some(owner) = &session.native {
                if selected.as_ref().is_some_and(|selected| selected != owner) {
                    return Err(problem("Retained session ancestry belongs to different native owners"));
                }
                selected = Some(owner.clone());
            }
            cursor = session.parent.as_deref();
        }
        Ok(selected)
    }
    pub(crate) fn bind_session_owner(
        &self,
        id: &str,
        owner: crate::daemon::NativeOwner,
    ) -> Result<()> {
        owner.validate_shape()?;
        self.update(|workspace| {
            let session = workspace
                .sessions
                .iter_mut()
                .find(|session| session.id == id)
                .ok_or_else(|| problem("Session not found"))?;
            if session.native.as_ref().is_some_and(|retained| retained != &owner) {
                return Err(problem("The browser session already belongs to another native owner"));
            }
            session.native = Some(owner);
            Ok(())
        })
    }
    pub(crate) fn open_project(&self, root: &Path) -> Result<Project> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(problem("Choose a project directory"));
        }
        self.update(|workspace| {
            if let Some(project) = workspace.projects.iter_mut().find(|p| p.root == root) {
                project.closed = false;
                return Ok(project.clone());
            }
            let project = Project {
                id: id()?,
                name: root.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                repository: root.clone(),
                closed: false,
                root,
            };
            workspace.sessions.push(Session {
                settings: crate::sessions::Settings::default(),
                id: id()?,
                conversation: id()?,
                run: id()?,
                native: None,
                project: project.id.clone(),
                parent: None,
                title: "New conversation".into(),
                closed: false,
            });
            workspace.projects.push(project.clone());
            Ok(project)
        })
    }
    pub(crate) fn adopt_project(&self, root: &Path) -> Result<Project> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(problem("The retained run project is no longer available"));
        }
        self.update(|workspace| {
            if let Some(project) = workspace.projects.iter_mut().find(|project| project.root == root)
            {
                project.closed = false;
                return Ok(project.clone());
            }
            let project = Project {
                id: id()?,
                name: root.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                repository: root.clone(),
                closed: false,
                root,
            };
            workspace.projects.push(project.clone());
            Ok(project)
        })
    }
    pub(crate) fn close_project(&self, id: &str) -> Result<()> {
        self.update(|state| {
            let project = state
                .projects
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| problem("Project not found"))?;
            project.closed = true;
            Ok(())
        })
    }
    pub(crate) fn nest(
        workspace: &Workspace,
        session: &Session,
        parent: Option<&str>,
    ) -> Result<()> {
        let mut cursor = parent;
        let root = workspace
            .projects
            .iter()
            .find(|p| p.id == session.project)
            .ok_or_else(|| problem("Unknown project"))?
            .root
            .canonicalize()?;
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = cursor {
            if id == session.id || !visited.insert(id) {
                return Err(problem("A session cannot be nested inside itself or its descendants"));
            }
            let ancestor = workspace
                .sessions
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| problem("Parent session not found"))?;
            if session
                .native
                .as_ref()
                .zip(ancestor.native.as_ref())
                .is_some_and(|(session, ancestor)| session != ancestor)
            {
                return Err(problem(
                    "Nested sessions cannot belong to different native owners",
                ));
            }
            let parent_root = workspace
                .projects
                .iter()
                .find(|p| p.id == ancestor.project)
                .ok_or_else(|| problem("Unknown parent project"))?
                .root
                .canonicalize()?;
            if parent_root != root {
                return Err(problem("Nested tabs must share the same canonical project root"));
            }
            cursor = ancestor.parent.as_deref();
        }
        Ok(())
    }
}

pub fn native_operation_belongs_to(key: &str, operation: &str) -> bool {
    let transport = format!("daemon:{operation}");
    key == transport || key.starts_with(&format!("{transport}:"))
}

fn recover_unsubmitted_native_operations(
    operations: &operation_store::OperationStore,
) -> Result<bool> {
    let mut after = None;
    let mut snapshot = None;
    let mut orphaned = Vec::new();
    loop {
        let page = operations.pending_page(after.as_deref(), snapshot.as_deref())?;
        snapshot.get_or_insert_with(|| page.snapshot.clone());
        for (id, input) in page.operations {
            let Some(command) = input["command"].as_str() else { continue };
            if !["send", "control", "session-settings", "improvements"].contains(&command) {
                continue;
            }
            let mut submitted = false;
            for candidate in [
                format!("daemon:{id}"),
                format!("daemon:{id}:workbench:create"),
                format!("daemon:{id}:workbench:queue"),
                format!("daemon:{id}:workbench:start"),
                format!("daemon:{id}:workbench:continue"),
            ] {
                if operations.get(&candidate)?.is_some() {
                    submitted = true;
                    break;
                }
            }
            if !submitted {
                orphaned.push(id);
            }
        }
        let Some(cursor) = page.cursor else { break };
        after = Some(cursor);
    }
    for id in &orphaned {
        operations.owner(id)?.settle(serde_json::json!({
            "error":"The operation stopped before native submission. It is safe to retry.",
            "recovered":true,
            "retryable":true,
            "submitted":false
        }))?;
    }
    Ok(!orphaned.is_empty())
}

#[cfg(test)]
mod tests;
