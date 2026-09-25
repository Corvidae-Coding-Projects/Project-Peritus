//! Narrow frontend build entry: exact package scripts and workflow commands, never arbitrary npm.

use super::policy_file;
use crate::error::Diagnostic;
use serde_json::{Value, json};
use std::path::Path;

fn scripts() -> Value {
    json!({
        "dev": "vite --host 127.0.0.1",
        "check": "svelte-check --tsconfig ./tsconfig.json",
        "build": "vite build",
        "test:unit": "vitest run --maxWorkers=2",
        "test:e2e": "playwright test"
    })
}

pub(super) fn validate_package(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let path = Path::new("webui/package.json");
    // Workspaces without a frontend (including policy fixtures) acquire no npm permission.
    if !root.join("webui").exists() {
        return false;
    }
    let Some(contents) = policy_file::read_regular(
        root,
        path,
        "WebUI package must be a regular repository file",
        "WebUI package",
        "restore webui/package.json without symlink components",
        diagnostics,
    ) else {
        return false;
    };
    let valid = serde_json::from_str::<Value>(&contents).is_ok_and(|package| {
        package["name"] == "peritus-webui"
            && package["private"] == true
            && package["scripts"] == scripts()
    });
    if !valid {
        diagnostics.push(Diagnostic::at(path,
            "WebUI package scripts differ from the reviewed frontend build entry",
            "retain the exact reviewed script map without lifecycle hooks or wrappers; review policy alongside intentional script changes"));
    }
    valid
}

pub(super) fn is_reviewed_command(path: &Path, script: &str) -> bool {
    path == Path::new(".github/workflows/webui.yml")
        && [
            "npm --prefix webui ci",
            "npm --prefix webui run check",
            "npm --prefix webui run test:unit",
            "npm --prefix webui run build",
            "npm --prefix webui run test:e2e",
            "npm --prefix webui exec --offline -- playwright install --with-deps chromium",
        ]
        .contains(&script.trim())
}

pub(super) fn is_generated_directory(path: &Path) -> bool {
    ["webui/node_modules", "webui/dist", "webui/test-results", "webui/playwright-report"]
        .iter()
        .any(|directory| path == Path::new(directory))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "peritus-webui-policy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("webui")).unwrap();
            Self(root)
        }
        fn write(&self, path: &str, text: &str) {
            fs::write(self.0.join(path), text).unwrap();
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn exact_scripts_reject_lifecycle_hooks_and_hidden_wrappers() {
        let fixture = Fixture::new();
        for mutation in [None, Some("pretest:e2e"), Some("check")] {
            let mut package = json!({"name":"peritus-webui","private":true,"scripts":scripts()});
            if let Some(key) = mutation {
                package["scripts"][key] = json!("cargo test");
            }
            fixture.write("webui/package.json", &package.to_string());
            let mut diagnostics = Vec::new();
            assert_eq!(validate_package(fixture.path(), &mut diagnostics), mutation.is_none());
            assert_eq!(diagnostics.is_empty(), mutation.is_none());
        }
    }

    #[test]
    fn frontend_exception_does_not_authorize_other_workflows_or_extra_commands() {
        let path = Path::new(".github/workflows/webui.yml");
        assert!(is_reviewed_command(path, "npm --prefix webui run check"));
        for script in [
            "npm test",
            "npm --prefix other run check",
            "npm --prefix webui run check; cargo test",
            "npm --prefix webui run check\ncargo test",
            "npm --prefix webui run check -- --help",
        ] {
            assert!(!is_reviewed_command(path, script));
        }
        assert!(!is_reviewed_command(
            Path::new(".github/workflows/other.yml"),
            "npm --prefix webui run check"
        ));
    }

    #[test]
    fn generated_exclusions_are_exact_frontend_output_directories() {
        assert!(is_generated_directory(Path::new("webui/node_modules")));
        assert!(is_generated_directory(Path::new("webui/dist")));
        for path in
            ["node_modules", "crates/node_modules", "webui/src", "webui/.cargo", "webui/dist-other"]
        {
            assert!(!is_generated_directory(Path::new(path)));
        }
    }

    #[test]
    fn command_permission_requires_reviewed_scripts_and_repository_working_directory() {
        use super::super::{workflow_command_policy::CommandPolicy, workflow_run::validate_step};
        use yaml_rust2::YamlLoader;
        for (permitted, directory, expected) in [
            (true, "", true),
            (false, "", false),
            (true, "\nworking-directory: another-project", false),
        ] {
            let yaml =
                YamlLoader::load_from_str(&format!("run: npm --prefix webui run check{directory}"))
                    .unwrap();
            let mut diagnostics = Vec::new();
            validate_step(
                yaml[0].as_hash().unwrap(),
                Path::new(".github/workflows/webui.yml"),
                "jobs.browser.steps[0]",
                CommandPolicy::new(true).with_webui_scripts(permitted),
                &mut diagnostics,
            );
            assert_eq!(diagnostics.is_empty(), expected);
        }
    }
}
