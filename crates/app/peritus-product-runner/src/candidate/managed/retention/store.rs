//! Local Git fetch shares immutable objects across task baselines without copying full packs.

use super::{ManagedBaseline, Manifest, failure, git, text, validate_object};
use crate::ProductRunnerError;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) struct Store {
    pub(super) root: PathBuf,
    format: &'static str,
    length: usize,
}

impl Store {
    pub(super) fn existing(owner: &Path, length: usize) -> Result<Self, ProductRunnerError> {
        let format = match length {
            40 => "sha1",
            64 => "sha256",
            _ => return Err(failure("invalid Git object format")),
        };
        let root = owner.join("peritus").join("retained-repositories").join(format);
        Ok(Self { root, format, length })
    }

    pub(super) fn prepare(owner: &Path, length: usize) -> Result<Self, ProductRunnerError> {
        let store = Self::existing(owner, length)?;
        let directory = owner.join("peritus");
        super::super::recovery::create_directory(&directory)?;
        let directory = directory.join("retained-repositories");
        super::super::recovery::create_directory(&directory)?;
        match fs::symlink_metadata(&store.root) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(failure("repository retention store is not an ordinary directory"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let temporary = tempfile::tempdir_in(&directory).map_err(failure)?;
                git(
                    temporary.path(),
                    &[
                        "init",
                        "--quiet",
                        "--bare",
                        "--template=",
                        &format!("--object-format={}", store.format),
                    ],
                    None,
                )?;
                let temporary = temporary.keep();
                fs::rename(&temporary, &store.root).map_err(|error| {
                    failure(format!(
                        "cannot publish repository retention store from {} to {}: {error}",
                        temporary.display(),
                        store.root.display()
                    ))
                })?;
                super::super::recovery::sync_directory(&directory)?;
            }
            Err(error) => return Err(failure(error)),
        }
        if text(git(&store.root, &["rev-parse", "--show-object-format"], None)?)? != store.format {
            return Err(failure("repository retention store has the wrong object format"));
        }
        Ok(store)
    }

    pub(super) fn fetch_repository(&self, root: &Path) -> Result<Vec<String>, ProductRunnerError> {
        let mut roots = std::collections::BTreeSet::new();
        for args in [
            vec!["for-each-ref", "--format=%(objectname)"],
            vec!["reflog", "show", "--all", "--format=%H"],
        ] {
            for object in text(git(root, &args, None)?)?.lines() {
                validate_object(object, self.length)?;
                roots.insert(object.to_owned());
            }
        }
        for object in &roots {
            git(
                root,
                &["update-ref", &format!("refs/peritus/task-baselines/{object}"), object],
                None,
            )?;
        }
        let source = super::super::git_path::local_repository(root)?;
        git(
            &self.root,
            &[
                "-c",
                "core.fsync=all",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.file.allow=always",
                "fetch",
                "--no-tags",
                "--update-shallow",
                "--no-write-fetch-head",
                "--no-auto-gc",
                &source,
                "+refs/peritus/task-baselines/*:refs/peritus/task-baselines/*",
            ],
            None,
        )?;
        for object in &roots {
            let retained = text(git(
                &self.root,
                &["rev-parse", "--verify", &format!("refs/peritus/task-baselines/{object}")],
                None,
            )?)?;
            if retained != *object {
                return Err(failure(
                    "independent repository backup is missing a required reference",
                ));
            }
        }
        Ok(roots.into_iter().collect())
    }

    pub(super) fn pin(&self, object: &str) -> Result<(), ProductRunnerError> {
        validate_object(object, self.length)?;
        git(
            &self.root,
            &[
                "-c",
                "core.fsync=all",
                "update-ref",
                &format!("refs/peritus/metadata/{object}"),
                object,
            ],
            None,
        )?;
        Ok(())
    }

    pub(super) fn load(&self, baseline: &ManagedBaseline) -> Result<Manifest, ProductRunnerError> {
        let object = baseline.retained.as_deref().ok_or_else(|| {
            failure("deleted nested repository has no independent baseline backup")
        })?;
        self.validate_store()?;
        validate_object(object, self.length)?;
        let manifest: Manifest =
            serde_json::from_slice(&git(&self.root, &["cat-file", "blob", object], None)?)
                .map_err(failure)?;
        if manifest.source_tree != baseline.tree || manifest.source_index != baseline.index {
            return Err(failure("independent repository backup does not match the task baseline"));
        }
        for object in &manifest.roots {
            validate_object(object, self.length)?;
        }
        for (path, entry) in &manifest.entries {
            super::validate_metadata_path(path)?;
            validate_object(&entry.object, self.length)?;
            if !matches!(entry.mode.as_str(), "100644" | "100755" | "120000") {
                return Err(failure("invalid retained Git metadata mode"));
            }
        }
        for path in manifest.directories.keys() {
            super::validate_metadata_path(path)?;
        }
        for path in manifest.entries.keys().chain(manifest.directories.keys()) {
            if manifest.entries.contains_key(path) && manifest.directories.contains_key(path) {
                return Err(failure("retained metadata path is both a file and a directory"));
            }
            for parent in Path::new(path).ancestors().skip(1) {
                if manifest.entries.contains_key(&super::super::git_path::tree_name(parent)?) {
                    return Err(failure("retained metadata path has a non-directory parent"));
                }
            }
        }
        super::linked::validate(&manifest)?;
        Ok(manifest)
    }

    fn validate_store(&self) -> Result<(), ProductRunnerError> {
        let retained =
            self.root.parent().ok_or_else(|| failure("retention store has no parent"))?;
        let peritus = retained.parent().ok_or_else(|| failure("retention store has no owner"))?;
        for directory in [peritus, retained, self.root.as_path()] {
            if !fs::symlink_metadata(directory).map_err(failure)?.is_dir() {
                return Err(failure("repository retention store is not an ordinary directory"));
            }
        }
        Ok(())
    }

    pub(super) fn initialize(
        &self,
        destination: &Path,
        manifest: &Manifest,
    ) -> Result<(), ProductRunnerError> {
        git(
            destination,
            &["init", "--quiet", "--template=", &format!("--object-format={}", self.format)],
            None,
        )?;
        let source = super::super::git_path::local_repository(&self.root)?;
        // Fetch immutable roots before restoring original refs, which may otherwise
        // appear dangling to Git's negotiation while the new object database is empty.
        for roots in manifest.roots.chunks(64) {
            let references = roots
                .iter()
                .map(|object| {
                    format!(
                        "refs/peritus/task-baselines/{object}:refs/peritus/task-baselines/{object}"
                    )
                })
                .collect::<Vec<_>>();
            let mut arguments = vec![
                "-c",
                "core.fsync=all",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.file.allow=always",
                "fetch",
                "--no-tags",
                "--update-shallow",
                "--no-write-fetch-head",
                "--no-auto-gc",
                &source,
            ];
            arguments.extend(references.iter().map(String::as_str));
            git(destination, &arguments, None)?;
        }
        Ok(())
    }

    pub(super) fn write_blob(
        &self,
        object: &str,
        destination: &Path,
    ) -> Result<fs::File, ProductRunnerError> {
        validate_object(object, self.length)?;
        let file = fs::File::create(destination).map_err(failure)?;
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(["cat-file", "blob", object])
            .stdout(file.try_clone().map_err(failure)?)
            .stderr(Stdio::piped())
            .output()
            .map_err(failure)?;
        if !output.status.success() {
            return Err(failure(String::from_utf8_lossy(&output.stderr)));
        }
        Ok(file)
    }
}
