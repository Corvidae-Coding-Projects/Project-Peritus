//! Stable exclusion of tool-generated dependency and build trees from task candidates.

use std::path::{Component, Path};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_language_build_and_cache_trees() {
        for path in [
            "crate/target/debug/app",
            "web/node_modules/library/index.js",
            "python/build/lib/package.py",
            "python/package.egg-info/PKG-INFO",
            "python/package.dist-info/METADATA",
            "python/.pytest_cache/v/cache/nodeids",
        ] {
            assert!(generated(Path::new(path)), "{path}");
        }
        assert!(!generated(Path::new("src/catalogue/build.py")));
        assert!(!generated(Path::new("src/package.py")));
    }
}
