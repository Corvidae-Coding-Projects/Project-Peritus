use super::manifest_date::CalendarDate;
use crate::error::Diagnostic;
use crate::model::{ArchitecturePolicy, CargoMetadata, CargoPackage};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub(super) struct ManifestContext<'policy> {
    pub(super) root: &'policy Path,
    pub(super) policy: &'policy ArchitecturePolicy,
    pub(super) cargo: &'policy CargoMetadata,
    pub(super) today: CalendarDate,
}

impl<'policy> ManifestContext<'policy> {
    pub(super) fn new(
        root: &'policy Path,
        policy: &'policy ArchitecturePolicy,
        cargo: &'policy CargoMetadata,
    ) -> Self {
        Self { root, policy, cargo, today: CalendarDate::today_utc() }
    }

    pub(super) fn package_class(&self, name: &str) -> Option<&str> {
        self.policy
            .packages
            .iter()
            .find(|package| package.name == name)
            .map(|package| package.verification_class.as_str())
    }

    pub(super) fn validate_source(
        &self,
        manifest: &Path,
        entry_id: &str,
        owning_crate: &str,
        source: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<PathBuf> {
        let relative = Path::new(source);
        if source.contains('\\') || !is_normal_relative(relative) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{entry_id}` has a non-normal repository source path `{source}`"),
                "use a non-empty relative path with `/` separators and no parent components",
            ));
            return None;
        }
        let absolute = self.root.join(relative);
        if !is_regular_without_symlinks(self.root, relative) {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("entry `{entry_id}` source is missing, non-regular, or reached by symlink"),
                "point the manifest at one checked-in regular source file",
            ));
            return None;
        }
        if self.policy.ignored_directories.iter().any(|ignored| relative.starts_with(ignored)) {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("entry `{entry_id}` source is hidden by an ignored repository prefix"),
                "move the governed source into the scanned workspace",
            ));
            return None;
        }
        let actual_owner = self.package_for_source(relative).map(|package| package.name.as_str());
        if actual_owner != Some(owning_crate) {
            diagnostics.push(Diagnostic::at(
                relative,
                format!(
                    "entry `{entry_id}` declares owner `{owning_crate}`, but Cargo ownership is {}",
                    actual_owner.unwrap_or("unowned")
                ),
                "use the most-specific registered workspace package that owns the source",
            ));
            return None;
        }
        Some(absolute)
    }

    fn package_for_source(&self, source: &Path) -> Option<&CargoPackage> {
        let members: BTreeSet<_> =
            self.cargo.workspace_members.iter().map(String::as_str).collect();
        self.cargo
            .packages
            .iter()
            .filter(|package| members.contains(package.id.as_str()))
            .filter_map(|package| {
                let root = package.manifest_path.parent()?.strip_prefix(self.root).ok()?;
                source.starts_with(root).then_some((root.components().count(), package))
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, package)| package)
    }
}

fn is_normal_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_)))
}

fn is_regular_without_symlinks(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = current.symlink_metadata() else { return false };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    current.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_file())
}
