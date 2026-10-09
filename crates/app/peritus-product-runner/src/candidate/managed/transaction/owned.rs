//! Nonces distinguish our preparation from foreign Git locks and adjacent user files.

use super::{Journal, Plan, ProductRunnerError, failure, recovery};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub(in crate::candidate::managed) enum Kind {
    Source,
    Index,
    Head,
    Lock,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::candidate::managed) struct Asset {
    pub path: PathBuf,
    kind: Kind,
    marker: Vec<u8>,
    sealed: Option<[u8; 32]>,
}

impl Asset {
    fn new(path: &Path, kind: Kind) -> Result<Self, ProductRunnerError> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(failure)?;
        let mut marker = String::from("peritus-discard-owned-v1:");
        for byte in nonce {
            write!(marker, "{byte:02x}").map_err(failure)?;
        }
        marker.push('\n');
        let marker = marker.into_bytes();
        Ok(Self { path: Self::normalized(path)?, kind, marker, sealed: None })
    }

    pub(super) fn normalized(path: &Path) -> Result<PathBuf, ProductRunnerError> {
        let parent = path
            .parent()
            .ok_or_else(|| failure("discard preparation has no parent"))?
            .canonicalize()
            .map_err(failure)?;
        let name = path.file_name().ok_or_else(|| failure("discard preparation has no name"))?;
        Ok(parent.join(name))
    }

    pub(super) fn validate(&self, plan: &Plan) -> Result<(), ProductRunnerError> {
        if !super::validation::absolute(&self.path)
            || self.marker.len() != 90
            || !self.marker.starts_with(b"peritus-discard-owned-v1:")
            || !self.marker[25..89].iter().all(u8::is_ascii_hexdigit)
            || self.marker.last() != Some(&b'\n')
        {
            return Err(failure("invalid discard preparation ownership"));
        }
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| failure("invalid discard preparation name"))?;
        let named = match self.kind {
            Kind::Source => name.starts_with(".peritus-restore-"),
            Kind::Index => name.starts_with("peritus-index-"),
            Kind::Head => name.starts_with("peritus-head-"),
            Kind::Lock => matches!(name, "index.lock" | "HEAD.lock"),
        };
        if !named || self.anchor(plan).is_none() {
            return Err(failure("discard preparation is outside its owned namespace"));
        }
        Ok(())
    }

    fn anchor<'a>(&self, plan: &'a Plan) -> Option<&'a Path> {
        let parent = self.path.parent()?;
        if self.kind == Kind::Source && parent.starts_with(&plan.root) {
            return Some(&plan.root);
        }
        plan.owners
            .iter()
            .find(|owner| {
                (self.kind != Kind::Source && parent == owner.as_path())
                    || parent.starts_with(owner.join("peritus/restoring"))
            })
            .map(PathBuf::as_path)
    }

    fn checked_parent(&self, plan: &Plan) -> Result<(), ProductRunnerError> {
        let anchor =
            self.anchor(plan).ok_or_else(|| failure("discard preparation lost its owner"))?;
        if anchor.canonicalize().map_err(failure)? != anchor {
            return Err(failure("discard metadata owner changed"));
        }
        let parent =
            self.path.parent().ok_or_else(|| failure("discard preparation has no parent"))?;
        let mut cursor = anchor.to_path_buf();
        for component in parent.strip_prefix(anchor).map_err(failure)?.components() {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err(failure("discard preparation traverses its owner"));
            }
            cursor.push(component);
            match fs::symlink_metadata(&cursor) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                _ => return Err(failure("discard preparation parent was replaced")),
            }
        }
        Ok(())
    }

    pub(super) fn check(&self, plan: &Plan) -> Result<(), ProductRunnerError> {
        self.checked_parent(plan)?;
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(failure(error)),
        };
        if self.kind == Kind::Lock {
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || read_marker(&self.path)? != self.marker
            {
                return Err(failure("a foreign Git lock replaced discard preparation"));
            }
            return Ok(());
        }
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failure("discard staging directory was replaced"));
        }
        let names = fs::read_dir(&self.path)
            .map_err(failure)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(failure)?;
        let allowed = match self.kind {
            Kind::Source => &[".owner", "source"][..],
            Kind::Index => &[".owner", "index", "index.lock"][..],
            Kind::Head => &[".owner", "HEAD"][..],
            Kind::Lock => return Err(failure("Git lock cannot be a staging directory")),
        };
        if names.iter().any(|name| !allowed.iter().any(|allowed| name == allowed)) {
            return Err(failure("discard staging contains an unowned entry; it was preserved"));
        }
        for name in &names {
            let metadata = fs::symlink_metadata(self.path.join(name)).map_err(failure)?;
            let source_link =
                self.kind == Kind::Source && name == "source" && metadata.file_type().is_symlink();
            if !source_link && (!metadata.is_file() || metadata.file_type().is_symlink()) {
                return Err(failure("discard staging contains a replaced entry; it was preserved"));
            }
        }
        if names.is_empty() {
            return Ok(());
        }
        let marker = read_marker(&self.path.join(".owner"))?;
        if marker != self.marker && !(names.len() == 1 && self.marker.starts_with(&marker)) {
            return Err(failure("discard staging ownership is invalid"));
        }
        if names.iter().any(|name| name != ".owner")
            && self.sealed != Some(super::validation_directory_digest(&self.path)?)
        {
            return Err(failure("discard staging is incomplete or changed; it was preserved"));
        }
        Ok(())
    }

    pub(super) fn seal(&mut self) -> Result<(), ProductRunnerError> {
        self.sealed = Some(super::validation_directory_digest(&self.path)?);
        Ok(())
    }

    pub(super) fn cleanup(&self, plan: &Plan) -> Result<(), ProductRunnerError> {
        self.check(plan)?;
        let result = if self.kind == Kind::Lock {
            fs::remove_file(&self.path)
        } else {
            // Remove known leaves individually. A directory or an unexpected child
            // inserted after validation cannot be traversed by recursive cleanup.
            if self.path.try_exists().map_err(failure)? {
                let leaves = match self.kind {
                    Kind::Source => &["source", ".owner"][..],
                    Kind::Index => &["index", "index.lock", ".owner"][..],
                    Kind::Head => &["HEAD", ".owner"][..],
                    Kind::Lock => &[][..],
                };
                for name in leaves {
                    match fs::remove_file(self.path.join(name)) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(failure(error)),
                    }
                }
            }
            fs::remove_dir(&self.path)
        };
        match result {
            Ok(()) => recovery::sync_directory(
                self.path.parent().ok_or_else(|| failure("discard preparation has no parent"))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(failure(error)),
        }
    }
}

pub(in crate::candidate::managed) fn own_directory(
    path: &Path,
    kind: Kind,
    journal: Option<&mut Journal>,
) -> Result<(), ProductRunnerError> {
    let Some(journal) = journal else { return Ok(()) };
    let asset = Asset::new(path, kind)?;
    let bytes = asset.marker.clone();
    journal.register(asset)?;
    let mut owner = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join(".owner"))
        .map_err(failure)?;
    owner.write_all(&bytes).and_then(|()| owner.sync_all()).map_err(failure)?;
    recovery::sync_directory(path)
}

fn read_marker(path: &Path) -> Result<Vec<u8>, ProductRunnerError> {
    let metadata = fs::symlink_metadata(path).map_err(failure)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(failure("discard owner marker is not an ordinary file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path).map_err(failure)?.take(257).read_to_end(&mut bytes).map_err(failure)?;
    Ok(bytes)
}

pub(in crate::candidate::managed) struct Lock {
    file: Option<fs::File>,
    asset: Asset,
    parent: PathBuf,
}

impl Lock {
    pub fn create(path: &Path, journal: Option<&mut Journal>) -> Result<Self, ProductRunnerError> {
        let asset = Asset::new(path, Kind::Lock)?;
        if let Some(journal) = journal {
            journal.register(asset.clone())?;
        }
        let parent = asset
            .path
            .parent()
            .ok_or_else(|| failure("Git lock has no parent"))?
            .canonicalize()
            .map_err(failure)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&parent).map_err(failure)?;
        temporary
            .write_all(&asset.marker)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(failure)?;
        // Publish a complete marker without ever replacing another Git operation's lock.
        let file = temporary.persist_noclobber(&asset.path).map_err(failure)?;
        recovery::sync_directory(&parent)?;
        Ok(Self { file: Some(file), asset, parent })
    }

    pub fn release(mut self) -> Result<(), ProductRunnerError> {
        drop(self.file.take());
        self.remove()
    }

    fn remove(&self) -> Result<(), ProductRunnerError> {
        if self
            .asset
            .path
            .parent()
            .ok_or_else(|| failure("Git lock has no parent"))?
            .canonicalize()
            .map_err(failure)?
            != self.parent
        {
            return Err(failure("Git lock parent changed"));
        }
        match read_marker(&self.asset.path) {
            Ok(marker) if marker == self.asset.marker => {
                fs::remove_file(&self.asset.path).map_err(failure)?;
                recovery::sync_directory(&self.parent)
            }
            Err(_) if !self.asset.path.try_exists().map_err(failure)? => Ok(()),
            _ => Err(failure("foreign Git lock was preserved")),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Err(error) = self.remove() {
            let _ = std::io::stderr()
                .lock()
                .write_all(format!("discard lock release failed: {error}\n").as_bytes());
        }
    }
}
