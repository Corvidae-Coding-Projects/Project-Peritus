//! Durable browser workspace and original operation outcomes.

use crate::{
    config::{Options, Preferences},
    error::{Result, problem},
    terminal::Terminal,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

const MAX_OPERATION_RECORDS: usize = 4_096;

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
    pub(crate) conversation: String,
    pub(crate) run: String,
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
    pub(crate) projects: Vec<Project>,
    pub(crate) sessions: Vec<Session>,
    pub(crate) operations: BTreeMap<String, Operation>,
    pub(crate) attachments: BTreeMap<String, crate::files::attachments::Attachment>,
}
pub struct App {
    pub(crate) options: Options,
    pub(crate) port: u16,
    pub(crate) token: String,
    pub(crate) workspace: Mutex<Workspace>,
    pub(crate) terminals: Mutex<BTreeMap<String, Arc<Terminal>>>,
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
    Ok(())
}
impl App {
    pub(crate) fn open(options: Options, port: u16) -> Result<Self> {
        let mut workspace = match std::fs::read(&options.state_file) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(workspace) => workspace,
                Err(error) => {
                    quarantine_malformed_state(&options.state_file);
                    eprintln!(
                        "peritus web: quarantined malformed workspace state and started with a fresh workspace: {error}"
                    );
                    Workspace::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Workspace::default(),
            Err(error) => return Err(error.into()),
        };
        let recovered_operations = recover_unsubmitted_native_operations(&mut workspace);
        let original_operation_count = workspace.operations.len();
        prune_operations(&mut workspace);
        if recovered_operations || workspace.operations.len() != original_operation_count {
            save(&options.state_file, &serde_json::to_vec_pretty(&workspace)?)?;
        }
        if !options.config_file.exists() {
            save(
                &options.config_file,
                toml::to_string_pretty(&Preferences::default()).map_err(problem)?.as_bytes(),
            )?;
        }
        let app = Self {
            options,
            port,
            token: id()?,
            workspace: Mutex::new(workspace),
            terminals: Mutex::new(BTreeMap::new()),
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
        prune_operations(&mut next);
        save(&self.options.state_file, &serde_json::to_vec_pretty(&next)?)?;
        *locked = next;
        drop(locked);
        Ok(result)
    }
    pub(crate) fn record_operation(&self, operation: String, input: Value) -> Result<()> {
        self.update(|workspace| insert_operation(workspace, operation, input))
    }
    pub(crate) fn retain_prepared_operation(&self, operation: &str, prepared: Value) -> Result<()> {
        self.update(|workspace| {
            let record = workspace
                .operations
                .get_mut(operation)
                .ok_or_else(|| problem("Original operation record missing"))?;
            if record.prepared.as_ref().is_some_and(|existing| existing != &prepared) {
                return Err(problem("Original operation execution context changed"));
            }
            record.prepared = Some(prepared);
            Ok(())
        })
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
                project: project.id.clone(),
                parent: None,
                title: "New conversation".into(),
                closed: false,
            });
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
        while let Some(id) = cursor {
            if id == session.id {
                return Err(problem("A session cannot be nested inside itself or its descendants"));
            }
            let ancestor = workspace
                .sessions
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| problem("Parent session not found"))?;
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

fn prune_operations(workspace: &mut Workspace) {
    while workspace.operations.len() > MAX_OPERATION_RECORDS {
        let Some(completed) = removable_completed_operation(workspace) else {
            break;
        };
        workspace.operations.remove(&completed);
    }
}

fn removable_completed_operation(workspace: &Workspace) -> Option<String> {
    workspace.operations.iter().find_map(|(id, record)| {
        (record.result.is_some() && !has_unresolved_native_parent(workspace, id))
            .then(|| id.clone())
    })
}

fn has_unresolved_native_parent(workspace: &Workspace, key: &str) -> bool {
    let Some(native) = key.strip_prefix("daemon:") else { return false };
    let parent = native.split(':').next().unwrap_or(native);
    workspace.operations.get(parent).is_some_and(|record| record.result.is_none())
}

pub fn native_operation_belongs_to(key: &str, operation: &str) -> bool {
    let transport = format!("daemon:{operation}");
    key == transport || key.starts_with(&format!("{transport}:"))
}

fn insert_operation(workspace: &mut Workspace, operation: String, input: Value) -> Result<()> {
    if workspace.operations.len() >= MAX_OPERATION_RECORDS
        && removable_completed_operation(workspace).is_none()
    {
        return Err(problem(
            "The operation ledger is full of unresolved actions. Resolve an original operation before starting another action.",
        ));
    }
    workspace.operations.insert(operation, Operation { input, prepared: None, result: None });
    Ok(())
}

fn recover_unsubmitted_native_operations(workspace: &mut Workspace) -> bool {
    let orphaned = workspace
        .operations
        .iter()
        .filter_map(|(id, record)| {
            let command = record.input["command"].as_str()?;
            let submitted =
                workspace.operations.keys().any(|key| native_operation_belongs_to(key, id));
            (record.result.is_none()
                && ["send", "control", "session-settings", "improvements"].contains(&command)
                && !submitted)
                .then(|| id.clone())
        })
        .collect::<Vec<_>>();
    for id in &orphaned {
        if let Some(record) = workspace.operations.get_mut(id) {
            record.result = Some(serde_json::json!({
                "error":"The operation stopped before native submission. It is safe to retry.",
                "recovered":true,
                "retryable":true,
                "submitted":false
            }));
        }
    }
    !orphaned.is_empty()
}

fn quarantine_malformed_state(path: &Path) {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let quarantine = parent.join(".quarantine");
    if let Err(error) = std::fs::create_dir_all(&quarantine) {
        eprintln!(
            "peritus web: could not create workspace quarantine {}: {error}",
            quarantine.display()
        );
        return;
    }
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    for suffix in 0_u32..=u32::MAX {
        let candidate = quarantine.join(format!("{file_name}.corrupt-{suffix}"));
        if candidate.exists() {
            continue;
        }
        if let Err(error) = std::fs::rename(path, &candidate) {
            eprintln!(
                "peritus web: could not quarantine malformed workspace state {}: {error}",
                path.display()
            );
        }
        return;
    }
}

#[cfg(test)]
mod tests;
