use super::{GitFixture, canonical, run_git};
use std::fs;
use std::process::{Command, Output};
use yaml_rust2::YamlLoader;

#[test]
fn unchanged_inputs_run_trusted_validation() {
    let fixture = ClassificationFixture::new(false, false);
    fixture.assert_changed(false);
}

#[test]
fn candidate_input_changes_require_bootstrap() {
    let fixture = ClassificationFixture::new(false, true);
    fixture.assert_changed(true);
}

#[test]
fn inherited_develop_input_changes_require_bootstrap() {
    let fixture = ClassificationFixture::new(true, false);
    fixture.assert_changed(true);
}

#[test]
fn restoring_checker_bytes_does_not_hide_base_drift() {
    let fixture = ClassificationFixture::new(true, true);
    fixture.assert_changed(true);
}

#[test]
fn unavailable_checker_fails_without_classification_even_when_candidate_changes() {
    let fixture = ClassificationFixture::new(false, true);
    let output = fixture.classify(&"f".repeat(40));
    assert!(!output.status.success());
    assert!(!fixture.output_path().exists());
}

struct ClassificationFixture {
    git: GitFixture,
    checker: String,
    base: String,
    candidate: String,
}

impl ClassificationFixture {
    fn new(base_changed: bool, candidate_changed: bool) -> Self {
        let git = GitFixture::new();
        git.write("xtask/src/lib.rs", "original checker\n");
        git.commit("checker");
        let checker = git.head();
        git.write("docs/base.md", "base change\n");
        if base_changed {
            git.write("xtask/src/lib.rs", "changed checker\n");
        }
        git.commit("base");
        let base = git.head();
        git.write("docs/candidate.md", "candidate change\n");
        if candidate_changed {
            let source = if base_changed { "original checker\n" } else { "changed checker\n" };
            git.write("xtask/src/lib.rs", source);
        }
        git.commit("candidate");
        let candidate = git.head();
        for (directory, revision) in [("base", &base), ("candidate", &candidate)] {
            run_git(&git.root, &["clone", "--quiet", "--no-checkout", ".", directory]);
            run_git(&git.root.join(directory), &["checkout", "--quiet", "--detach", revision]);
        }
        Self { git, checker, base, candidate }
    }

    fn output_path(&self) -> std::path::PathBuf {
        self.git.root.join("classification-output")
    }

    fn classify(&self, checker: &str) -> Output {
        let workflow = YamlLoader::load_from_str(&canonical()).expect("authority workflow");
        let script = workflow[0]["jobs"]["protected-input-classification"]["steps"][2]["run"]
            .as_str()
            .expect("classification script");
        Command::new("bash")
            .args(["--noprofile", "--norc", "-c", script])
            .current_dir(&self.git.root)
            .env("CHECKER_SHA", checker)
            .env("EVENT_SHA", checker)
            .env("BASE_SHA", &self.base)
            .env("CANDIDATE_SHA", &self.candidate)
            .env("EVENT_NAME", "pull_request_target")
            .env("DEFAULT_BRANCH", "main")
            .env("EVENT_REF", "refs/heads/main")
            .env("WORKFLOW_REF", "test/repo/.github/workflows/formal-authority.yml@refs/heads/main")
            .env("GITHUB_REPOSITORY", "test/repo")
            .env("BASE_REPOSITORY", "test/repo")
            .env("BASE_REPOSITORY_ID", "1")
            .env("REPOSITORY_ID", "1")
            .env("BASE_REF", "develop")
            .env("GITHUB_OUTPUT", self.output_path())
            .output()
            .expect("execute canonical classification script")
    }

    fn assert_changed(&self, changed: bool) {
        let output = self.classify(&self.checker);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            fs::read_to_string(self.output_path()).expect("classification output"),
            format!("changed={changed}\n")
        );
        if changed {
            assert!(String::from_utf8_lossy(&output.stdout).contains("bootstrap required"));
        }
    }
}
