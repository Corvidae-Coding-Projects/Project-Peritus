//! Stable exclusion of tool-generated dependency and build trees from task candidates.

use std::path::{Component, Path};

/// Reports whether a relative source path belongs to generated dependency/build output.
pub fn generated(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let Some(name) = name.to_str() else {
            return false;
        };
        matches!(
            name,
            "target"
                | "node_modules"
                | ".venv"
                | "venv"
                | "__pycache__"
                | "build"
                | ".eggs"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
        ) || name.ends_with(".egg-info")
            || name.ends_with(".dist-info")
    })
}
