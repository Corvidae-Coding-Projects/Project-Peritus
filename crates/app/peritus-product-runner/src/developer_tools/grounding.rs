//! Deterministic evidence that a model inspected the managed repository.

use std::{collections::BTreeSet, fmt::Write as _, path::PathBuf};

#[derive(Default)]
pub struct GroundingEvidence {
    list_calls: u32,
    search_calls: u32,
    read_paths: BTreeSet<PathBuf>,
    mutation_paths: BTreeSet<PathBuf>,
    root_observed_empty: bool,
}

impl GroundingEvidence {
    pub const fn record_list(&mut self, path: &str, entries: usize) {
        self.list_calls = self.list_calls.saturating_add(1);
        if path.is_empty() && entries == 0 {
            self.root_observed_empty = true;
        }
    }

    pub const fn record_search(&mut self) {
        self.search_calls = self.search_calls.saturating_add(1);
    }

    pub fn record_read(&mut self, path: &str) {
        self.read_paths.insert(PathBuf::from(path));
    }

    pub fn record_mutation(&mut self, path: &str) {
        self.mutation_paths.insert(PathBuf::from(path));
    }

    pub fn ensure_mutation_allowed(&self, path: &str, exists: bool) -> Result<(), String> {
        self.validate().map_err(str::to_owned)?;
        if exists && !self.read_paths.contains(&PathBuf::from(path)) {
            return Err(format!("read the existing target before mutating it: {path}"));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.list_calls == 0 {
            return Err("repository grounding requires a successful workspace listing");
        }
        if self.read_paths.is_empty() && !self.root_observed_empty {
            return Err("repository grounding requires reading an observed repository file");
        }
        Ok(())
    }

    pub fn markdown(&self) -> String {
        let mut text = String::from("\n## Repository grounding evidence\n\n");
        let _ = write!(
            text,
            "- Workspace listings: {}\n- Targeted searches: {}\n",
            self.list_calls, self.search_calls,
        );
        if self.root_observed_empty {
            text.push_str("- The workspace root was observed to be empty.\n");
        }
        if !self.read_paths.is_empty() {
            text.push_str("- Files read:\n");
            for path in &self.read_paths {
                let _ = write!(text, "  - `{}`", path.display());
                text.push('\n');
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_target_requires_repository_and_target_observation() {
        let mut evidence = GroundingEvidence::default();
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_err());
        evidence.record_list("", 2);
        evidence.record_read("Cargo.toml");
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_err());
        evidence.record_read("src/lib.rs");
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_ok());
    }

    #[test]
    fn observed_empty_repository_allows_new_files() {
        let mut evidence = GroundingEvidence::default();
        evidence.record_list("", 0);
        assert!(evidence.ensure_mutation_allowed("src/main.rs", false).is_ok());
    }
}
