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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
    pub(crate) name: String,
    pub(crate) repository: PathBuf,
    #[serde(default)]
    pub(crate) closed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub(crate) settings: crate::sessions::Settings,
    pub(crate) id: String,
    pub(crate) project: String,
    pub(crate) parent: Option<String>,
    pub(crate) title: String,
    pub(crate) closed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Operation {
    pub(crate) input: Value,
    pub(crate) result: Option<Value>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub(crate) projects: Vec<Project>,
    pub(crate) sessions: Vec<Session>,
    #[serde(default)]
    pub(crate) operations: BTreeMap<String, Operation>,
    #[serde(default)]
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
        let workspace = match std::fs::read(&options.state_file) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Workspace::default(),
            Err(error) => return Err(error.into()),
        };
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
        save(&self.options.state_file, &serde_json::to_vec_pretty(&next)?)?;
        *locked = next;
        drop(locked);
        Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nesting_rejects_other_roots_and_cycles() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let projects = vec![
            Project {
                id: "a".into(),
                name: "a".into(),
                root: a.path().into(),
                repository: a.path().into(),
                closed: false,
            },
            Project {
                id: "b".into(),
                name: "b".into(),
                root: b.path().into(),
                repository: b.path().into(),
                closed: false,
            },
        ];
        let one = Session {
            settings: crate::sessions::Settings::default(),
            id: "one".into(),
            project: "a".into(),
            parent: None,
            title: "one".into(),
            closed: false,
        };
        let two = Session { id: "two".into(), project: "b".into(), ..one.clone() };
        let child = Session { id: "child".into(), parent: Some("one".into()), ..one.clone() };
        let workspace = Workspace {
            projects,
            sessions: vec![one.clone(), two, child.clone()],
            ..Workspace::default()
        };
        assert!(App::nest(&workspace, &one, Some("two")).is_err());
        assert!(App::nest(&workspace, &one, Some("child")).is_err());
        assert!(App::nest(&workspace, &child, Some("one")).is_ok());
    }
}
