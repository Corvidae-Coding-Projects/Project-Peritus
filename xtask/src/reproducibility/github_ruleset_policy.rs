use super::policy_file;
use crate::error::Diagnostic;
use std::path::Path;

pub(super) const PATH: &str = "docs/formal-governance-ruleset.template.json";
const REVIEWED: &str = include_str!("../../../docs/formal-governance-ruleset.template.json");

pub(super) fn validate(root: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some(contents) = policy_file::read_regular(
        root,
        Path::new(PATH),
        "the GitHub ruleset activation template is missing, non-regular, or symbolic",
        "GitHub ruleset activation template",
        "restore the reviewed regular template before claiming protected Gate A enforcement",
        diagnostics,
    ) else {
        return;
    };
    if contents != REVIEWED {
        diagnostics.push(Diagnostic::at(
            PATH,
            "the GitHub ruleset activation template differs from its reviewed Team definition",
            "restore the canonical template byte-for-byte; changes require independent review and a coordinated ruleset update",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{PATH, REVIEWED, validate};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn accepts_only_the_exact_regular_ruleset_template() {
        let fixture = Fixture::new();
        fixture.write(REVIEWED);
        let mut diagnostics = Vec::new();
        validate(&fixture.root, &mut diagnostics);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");

        fixture.write(
            &REVIEWED.replace("\"enforcement\": \"active\"", "\"enforcement\": \"disabled\""),
        );
        validate(&fixture.root, &mut diagnostics);
        assert!(diagnostics.iter().any(|item| item.message().contains("differs")));
    }

    #[test]
    fn reviewed_template_requires_the_strict_gate_a_status_without_bypass() {
        let document: serde_json::Value =
            serde_json::from_str(REVIEWED).expect("reviewed ruleset template must be valid JSON");
        assert_eq!(document["name"], "Project Peritus Gate A");
        assert_eq!(document["target"], "branch");
        assert_eq!(document["enforcement"], "active");
        assert_eq!(document["bypass_actors"], serde_json::json!([]));
        assert_eq!(document["conditions"].as_object().map(serde_json::Map::len), Some(1));
        let rules = document["rules"].as_array().expect("rules must be an array");
        assert_eq!(
            rules.iter().filter_map(|rule| rule["type"].as_str()).collect::<Vec<_>>(),
            ["deletion", "non_fast_forward", "pull_request", "required_status_checks"]
        );
        let status = rules
            .iter()
            .find(|rule| rule["type"] == "required_status_checks")
            .expect("Gate A status rule must exist");
        assert_eq!(status["parameters"]["do_not_enforce_on_create"], false);
        assert_eq!(status["parameters"]["strict_required_status_checks_policy"], true);
        assert_eq!(
            status["parameters"]["required_status_checks"],
            serde_json::json!([{ "context": "Gate A" }])
        );
        assert!(!REVIEWED.contains("workflows"));
        assert!(!REVIEWED.contains("__"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symbolic_ruleset_template() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let alternate = fixture.root.join("alternate.json");
        fs::write(&alternate, REVIEWED).expect("alternate template must be writable");
        let path = fixture.root.join(PATH);
        fs::create_dir_all(path.parent().expect("template path must have a parent"))
            .expect("template directory must be creatable");
        symlink(alternate, path).expect("template symlink must be creatable");

        let mut diagnostics = Vec::new();
        validate(&fixture.root, &mut diagnostics);
        assert!(diagnostics.iter().any(|item| item.message().contains("symbolic")));
    }

    #[test]
    fn missing_template_retains_the_reproducibility_diagnostic() {
        let fixture = Fixture::new();
        let mut diagnostics = Vec::new();

        validate(&fixture.root, &mut diagnostics);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message().contains("missing, non-regular, or symbolic"));
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "peritus-ruleset-policy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).expect("fixture root must be creatable");
            Self { root }
        }

        fn write(&self, contents: &str) {
            let path = self.root.join(PATH);
            fs::create_dir_all(path.parent().expect("template path must have a parent"))
                .expect("template directory must be creatable");
            fs::write(path, contents).expect("template must be writable");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("fixture root must be removable");
        }
    }
}
