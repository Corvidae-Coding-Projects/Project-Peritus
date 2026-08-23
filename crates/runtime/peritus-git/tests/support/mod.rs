use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use peritus_git::{GitRepository, RepositoryOptions};
use tempfile::TempDir;

pub struct RepositoryFixture {
    pub temporary: TempDir,
    pub root: PathBuf,
}

impl RepositoryFixture {
    pub fn sha1() -> Self {
        Self::new(None).expect("initialize SHA-1 repository")
    }

    pub fn new(format: Option<&str>) -> Option<Self> {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("source");
        let mut arguments = vec!["init", "--quiet"];
        let format_argument;
        if let Some(format) = format {
            format_argument = format!("--object-format={format}");
            arguments.push(&format_argument);
        }
        arguments.push(root.to_str().expect("UTF-8 temporary path"));
        if !git(temporary.path(), &arguments).status.success() {
            return None;
        }
        checked_git(&root, &["config", "user.name", "Peritus Test"]);
        checked_git(&root, &["config", "user.email", "peritus@example.invalid"]);
        std::fs::write(root.join("tracked.txt"), b"baseline\n").expect("write baseline");
        checked_git(&root, &["add", "--", "tracked.txt"]);
        checked_git(&root, &["commit", "--quiet", "-m", "baseline"]);
        Some(Self { temporary, root })
    }

    pub fn open(&self) -> GitRepository {
        GitRepository::open(RepositoryOptions::new(&self.root)).expect("open repository")
    }

    pub fn worktree_path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }
}

pub fn checked_git(cwd: &Path, arguments: &[&str]) -> String {
    let output = git(cwd, arguments);
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 Git output").trim().to_owned()
}

pub fn git(cwd: &Path, arguments: &[&str]) -> Output {
    Command::new("git")
        .current_dir(cwd)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("launch git")
}
