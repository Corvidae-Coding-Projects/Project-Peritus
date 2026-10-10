//! Stable directory pages; filesystem scans occur only when a new inventory is requested.

use crate::error::{Result, problem};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    name: String,
    path: String,
    directory: bool,
    symlink: bool,
    bytes: u64,
}
struct Inventory {
    id: String,
    entries: Vec<Entry>,
}
#[derive(Default)]
pub struct DirectoryCache(Mutex<BTreeMap<(PathBuf, PathBuf), Arc<Inventory>>>);

impl DirectoryCache {
    pub(crate) fn page(
        &self,
        root: &Path,
        relative: &str,
        offset: usize,
        identity: &str,
    ) -> Result<Value> {
        let root = root.canonicalize()?;
        let directory = super::resolve(&root, relative)?;
        let key = (root.clone(), directory.clone());
        let inventory = if identity.is_empty() && offset == 0 {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&directory)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let path = entry
                    .path()
                    .strip_prefix(&root)
                    .map_err(problem)?
                    .to_string_lossy()
                    .into_owned();
                entries.push(Entry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path,
                    directory: metadata.is_dir(),
                    symlink: entry.file_type()?.is_symlink(),
                    bytes: metadata.len(),
                });
            }
            entries.sort_by(|a, b| {
                b.directory
                    .cmp(&a.directory)
                    .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then(a.name.cmp(&b.name))
            });
            let inventory = Arc::new(Inventory { id: crate::state::id()?, entries });
            self.0.lock().map_err(problem)?.insert(key, Arc::clone(&inventory));
            inventory
        } else {
            let inventory =
                self.0.lock().map_err(problem)?.get(&key).cloned().ok_or_else(|| {
                    problem("Directory inventory expired; refresh this directory")
                })?;
            if inventory.id != identity {
                return Err(problem("Directory inventory changed; refresh this directory"));
            }
            inventory
        };
        let total = inventory.entries.len();
        let end = offset.saturating_add(250).min(total);
        if offset > total {
            return Err(problem("Directory offset is outside this inventory"));
        }
        Ok(json!({"entries": &inventory.entries[offset..end], "total":total,
            "next":(end < total).then_some(end), "inventory": inventory.id}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_uses_stable_inventory_and_refresh_rejects_old_cursors() {
        let root = tempfile::tempdir().expect("root");
        for index in 0..600 {
            std::fs::write(root.path().join(format!("entry-{index:04}")), "").expect("entry");
        }
        let cache = DirectoryCache::default();
        let first = cache.page(root.path(), "", 0, "").expect("first");
        let identity = first["inventory"].as_str().expect("identity");
        std::fs::remove_file(root.path().join("entry-0300")).expect("delete entry");
        std::fs::write(root.path().join("entry-0000-new"), "new").expect("insert entry");
        let next = cache.page(root.path(), "", 250, identity).expect("continuation");
        assert_eq!(next["entries"][0]["name"], "entry-0250");
        assert_eq!(next["entries"][50]["name"], "entry-0300");
        cache.page(root.path(), "", 0, "").expect("refresh");
        assert!(cache.page(root.path(), "", 500, identity).is_err());
        assert!(cache.page(root.path(), "..", 0, "").is_err());
    }

    #[test]
    fn equivalent_root_aliases_share_exact_relative_paths_and_inventory() {
        let holder = tempfile::tempdir().expect("holder");
        let root = holder.path().join("project");
        let anchor = holder.path().join("anchor");
        std::fs::create_dir(&root).expect("project");
        std::fs::create_dir(&anchor).expect("alias anchor");
        let alias = anchor.join("..").join("project");
        for index in 0..251 {
            std::fs::write(root.join(format!("entry-{index:04}")), "").expect("entry");
        }
        let cache = DirectoryCache::default();
        let first = cache.page(&alias, "", 0, "").expect("noncanonical root");
        assert_eq!(first["entries"][0]["path"], "entry-0000");
        let identity = first["inventory"].as_str().expect("inventory");
        let canonical = root.canonicalize().expect("canonical project");
        let next = cache.page(&canonical, "", 250, identity).expect("equivalent alias");
        assert_eq!(next["entries"][0]["path"], "entry-0250");
        assert_eq!(next["inventory"], identity);
        assert!(cache.page(&alias, "../anchor", 0, "").is_err(), "retain project confinement");
    }

    #[test]
    fn nested_project_inventories_keep_their_own_relative_path_base() {
        let root = tempfile::tempdir().expect("project");
        let nested = root.path().join("nested");
        std::fs::create_dir(&nested).expect("nested project");
        std::fs::write(nested.join("file.txt"), "file").expect("entry");
        let cache = DirectoryCache::default();
        let parent_page = cache.page(root.path(), "nested", 0, "").expect("parent project");
        let parent_id = parent_page["inventory"].as_str().expect("parent inventory");
        let nested_page = cache.page(&nested, "", 0, "").expect("nested project");
        let nested_id = nested_page["inventory"].as_str().expect("nested inventory");
        assert_eq!(
            parent_page["entries"][0]["path"],
            Path::new("nested").join("file.txt").to_string_lossy().as_ref()
        );
        assert_eq!(nested_page["entries"][0]["path"], "file.txt");
        assert_eq!(
            cache.page(root.path(), "nested", 0, parent_id).expect("parent snapshot retained"),
            parent_page
        );
        assert_eq!(
            cache.page(&nested, "", 0, nested_id).expect("nested snapshot retained"),
            nested_page
        );
        assert!(cache.page(&nested, "", 0, parent_id).is_err(), "other root cannot borrow cursor");
        assert!(
            cache.page(root.path(), "nested", 0, nested_id).is_err(),
            "nested cursor stays scoped"
        );
    }
}
