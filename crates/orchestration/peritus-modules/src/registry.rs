//! Atomic installed-package publication and persistent per-user enablement.
use crate::{Definition, Error, invalid, publish};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(default)]
    modules: BTreeMap<String, Selection>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    enabled: bool,
    digest: String,
    installed: bool,
}
/// One registry root; package bytes are immutable and selection updates are serialized.
#[derive(Clone, Debug)]
pub struct Registry {
    root: PathBuf,
}
/// Per-user module storage, independent of project files and provider credentials.
pub fn data_root() -> Result<PathBuf, Error> {
    let root = if cfg!(windows) {
        absolute_environment("LOCALAPPDATA").map(|p| p.join("Peritus"))
    } else if cfg!(target_os = "macos") {
        absolute_environment("HOME").map(|p| p.join("Library/Application Support/Peritus"))
    } else {
        absolute_environment("XDG_DATA_HOME")
            .or_else(|| absolute_environment("HOME").map(|p| p.join(".local/share")))
            .map(|p| p.join("peritus"))
    };
    root.map(|p| p.join("modules")).ok_or_else(|| invalid("module data directory is unavailable"))
}
fn absolute_environment(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute())
}
impl Registry {
    /// Uses the current user's private module directory.
    pub fn for_user() -> Result<Self, Error> {
        Ok(Self::new(data_root()?))
    }
    /// Uses an explicit registry root, useful for embedders and disposable operation.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    /// Registry directory, never a workspace write destination.
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn settings(&self) -> Result<Settings, Error> {
        match fs::read(self.root.join("settings.json")) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn lock(&self) -> Result<fs::File, Error> {
        fs::create_dir_all(&self.root)?;
        let f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("registry.lock"))?;
        fs4::FileExt::lock(&f)?;
        Ok(f)
    }
    fn save(&self, value: &Settings) -> Result<(), Error> {
        publish(&self.root.join("settings.json"), &serde_json::to_vec_pretty(value)?)
    }
    fn bundled() -> Result<Vec<Definition>, Error> {
        Ok(vec![Definition::bundled(include_str!("../bundled/source-brief/module.toml"))?])
    }
    /// Lists all selected installed/bundled modules and reports corrupt packages individually.
    pub fn list(&self) -> Result<Value, Error> {
        let settings = self.settings()?;
        let mut entries = BTreeMap::new();
        for definition in Self::bundled()? {
            let id = definition.manifest.id.clone();
            let enabled = settings.modules.get(&id).map_or(true, |s| s.enabled);
            entries.insert(id,json!({"id":definition.manifest.id,"name":definition.manifest.name,"version":definition.manifest.version,"description":definition.manifest.description,"digest":definition.digest,"bundled":true,"enabled":enabled}));
        }
        for (id, selection) in &settings.modules {
            if !selection.installed {
                continue;
            }
            let entry = match self.load(id, false) {
                Ok(d) => {
                    json!({"id":id,"name":d.manifest.name,"version":d.manifest.version,"description":d.manifest.description,"digest":d.digest,"bundled":false,"enabled":selection.enabled,"kind":if d.manifest.code.is_some(){"native-rust"}else{"workflow"}})
                }
                Err(e) => {
                    json!({"id":id,"digest":selection.digest,"enabled":selection.enabled,"error":e.to_string()})
                }
            };
            entries.insert(id.clone(), entry);
        }
        Ok(
            json!({"modules":entries.into_values().collect::<Vec<_>>(),"directory":self.root,"format":"native-rust-and-workflow-v1"}),
        )
    }
    /// Loads a selected exact package and verifies its complete declared resource identity.
    pub fn load(&self, id: &str, require_enabled: bool) -> Result<Definition, Error> {
        if !crate::definition::identifier(id) {
            return Err(invalid("invalid module identifier"));
        }
        let settings = self.settings()?;
        if let Some(selection) = settings.modules.get(id) {
            if require_enabled && !selection.enabled {
                return Err(invalid("module is disabled; enable it explicitly before running"));
            }
            if selection.installed {
                check_digest(&selection.digest)?;
                let d = Definition::from_directory(
                    &self.root.join("packages").join(id).join(&selection.digest),
                )?;
                if d.manifest.id != id || d.digest != selection.digest {
                    return Err(invalid(
                        "installed module bytes changed; reinstall and explicitly enable the intended version",
                    ));
                }
                return Ok(d);
            }
        }
        Self::bundled()?
            .into_iter()
            .find(|d| d.manifest.id == id)
            .ok_or_else(|| invalid("module is not installed or bundled"))
    }
    /// Installs an inert local package, initially disabled. Updates never reuse old enablement.
    pub fn install(&self, source: &Path) -> Result<Value, Error> {
        let original = Definition::from_directory(source)?;
        if Self::bundled()?.iter().any(|d| d.manifest.id == original.manifest.id) {
            return Err(invalid("installed package cannot replace a bundled module identity"));
        }
        let _lock = self.lock()?;
        let mut settings = self.settings()?;
        let packages = self.root.join("packages").join(&original.manifest.id);
        fs::create_dir_all(&packages)?;
        let temp = tempfile::tempdir_in(&packages)?;
        let root = original
            .root
            .as_deref()
            .ok_or_else(|| invalid("installation requires a local package"))?;
        fs::copy(
            crate::definition::checked_file(root, "module.toml")?,
            temp.path().join("module.toml"),
        )?;
        for resource in &original.manifest.resources {
            let path = original.resource(resource)?;
            let destination = temp.path().join(resource);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, destination)?;
        }
        let staged = Definition::from_directory(temp.path())?;
        if staged.digest != original.digest {
            return Err(invalid("module changed while installing; prior selection was preserved"));
        }
        // Sync copied bytes and their directory entries before selecting the package.
        for resource in std::iter::once("module.toml")
            .chain(staged.manifest.resources.iter().map(String::as_str))
        {
            let path = temp.path().join(resource);
            fs::File::open(&path)?.sync_all()?;
            #[cfg(unix)]
            {
                let mut parent = path.parent();
                while let Some(directory) = parent {
                    fs::File::open(directory)?.sync_all()?;
                    if directory == temp.path() {
                        break;
                    }
                    parent = directory.parent();
                }
            }
        }
        let destination = packages.join(&staged.digest);
        let mut quarantine = None;
        let exists = match fs::symlink_metadata(&destination) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        let valid_existing = exists
            && Definition::from_directory(&destination).is_ok_and(|d| d.digest == staged.digest);
        if exists && !valid_existing {
            // Retain damaged bytes for diagnosis while restoring this exact immutable identity.
            let directory =
                tempfile::Builder::new().prefix("corrupt-").tempdir_in(&packages)?.keep();
            let retained = directory.join("package");
            fs::rename(&destination, &retained)?;
            quarantine = Some(retained);
        }
        if !valid_existing {
            if let Err(failure) = fs::rename(temp.path(), &destination) {
                if let Some(retained) = &quarantine {
                    fs::rename(retained, &destination).map_err(|rollback| invalid(format!("package publication failed: {failure}; damaged package restoration failed: {rollback}; retained at {}",retained.display())))?;
                }
                return Err(failure.into());
            }
        }
        #[cfg(unix)]
        {
            fs::File::open(&packages)?.sync_all()?;
            if let Some(parent) = quarantine.as_ref().and_then(|p| p.parent()) {
                fs::File::open(parent)?.sync_all()?;
            }
            if let Some(parent) = packages.parent() {
                fs::File::open(parent)?.sync_all()?;
            }
        }
        settings.modules.insert(
            staged.manifest.id.clone(),
            Selection { enabled: false, digest: staged.digest.clone(), installed: true },
        );
        self.save(&settings)?;
        Ok(
            json!({"installed":staged.manifest.id,"version":staged.manifest.version,"digest":staged.digest,"enabled":false,"directory":destination,"quarantine":quarantine}),
        )
    }
    /// Selects enablement without granting additional tool permissions.
    pub fn enable(&self, id: &str, enabled: bool) -> Result<Value, Error> {
        let _lock = self.lock()?;
        let d = self.load(id, false)?;
        let mut settings = self.settings()?;
        settings.modules.insert(
            id.to_owned(),
            Selection { enabled, digest: d.digest.clone(), installed: !d.bundled },
        );
        self.save(&settings)?;
        Ok(
            json!({"id":id,"enabled":enabled,"digest":d.digest,"permissions":"agent host tools follow live permissions; native code runs as the current OS account"}),
        )
    }
    /// Removes an installed selection. Immutable bytes remain for pinned historical jobs.
    pub fn remove(&self, id: &str) -> Result<Value, Error> {
        let _lock = self.lock()?;
        let mut settings = self.settings()?;
        if !settings.modules.get(id).is_some_and(|s| s.installed) {
            return Err(invalid(
                "only installed modules can be removed; bundled modules can be disabled",
            ));
        }
        settings.modules.remove(id);
        self.save(&settings)?;
        Ok(json!({"removed":id,"retained":"immutable package bytes and historical job outputs"}))
    }
}
fn check_digest(value: &str) -> Result<(), Error> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("invalid installed module digest"));
    }
    Ok(())
}
