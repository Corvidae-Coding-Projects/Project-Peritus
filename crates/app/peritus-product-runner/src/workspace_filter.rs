//! Stable exclusion of tool-generated dependency and build trees from task candidates.

use std::path::{Component, Path};

pub fn generated(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(name)
                if matches!(
                    name.to_str(),
                    Some("target" | "node_modules" | ".venv" | "__pycache__")
                )
        )
    })
}
