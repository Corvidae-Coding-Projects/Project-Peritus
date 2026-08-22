//! Explicit allowlisted environment for isolated Git subprocesses.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

const FIXED_NAME: &str = "Peritus Test";
const FIXED_EMAIL: &str = "peritus-test@example.invalid";
const FIXED_DATE: &str = "2000-01-01T00:00:00Z";

pub(super) struct GitCommandContext<'a> {
    pub(super) git_program: &'a OsStr,
    pub(super) repository_root: &'a Path,
    pub(super) hooks_root: &'a Path,
    pub(super) global_config: &'a Path,
    pub(super) process_temp: &'a Path,
    pub(super) bare: bool,
}

pub(super) fn isolated_git_command<I, S, P>(
    context: &GitCommandContext<'_>,
    arguments: I,
    parent_environment: P,
) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    P: IntoIterator<Item = (OsString, OsString)>,
{
    let git_directory = if context.bare {
        context.repository_root.to_owned()
    } else {
        context.repository_root.join(".git")
    };
    let work_tree = (!context.bare).then_some(context.repository_root);
    let environment = GitEnvironment::from_parent(
        parent_environment,
        &git_directory,
        work_tree,
        context.global_config,
        context.process_temp,
    );
    let mut command = Command::new(context.git_program);
    command.arg("-C").arg(context.repository_root);
    command.arg("-c").arg(format!("user.name={FIXED_NAME}"));
    command.arg("-c").arg(format!("user.email={FIXED_EMAIL}"));
    command.arg("-c").arg("credential.helper=");
    command.arg("-c").arg("core.askPass=");
    command.arg("-c").arg("commit.gpgsign=false");
    command.arg("-c").arg("tag.gpgsign=false");
    command.arg("-c").arg("core.autocrlf=false");
    command.arg("-c").arg("core.filemode=false");
    let mut hooks_argument = OsString::from("core.hooksPath=");
    hooks_argument.push(context.hooks_root);
    command.arg("-c").arg(hooks_argument);
    command.args(arguments);
    environment.apply(&mut command);
    command
}

#[derive(Debug)]
struct GitEnvironment {
    values: BTreeMap<OsString, OsString>,
}

impl GitEnvironment {
    pub(super) fn from_parent(
        parent: impl IntoIterator<Item = (OsString, OsString)>,
        git_directory: &Path,
        work_tree: Option<&Path>,
        global_config: &Path,
        process_temp: &Path,
    ) -> Self {
        let parent: BTreeMap<_, _> = parent.into_iter().collect();
        let mut values = BTreeMap::new();
        copy_launch_variable(&parent, &mut values, "PATH");
        #[cfg(windows)]
        {
            copy_launch_variable(&parent, &mut values, "PATHEXT");
            copy_launch_variable(&parent, &mut values, "SystemRoot");
            copy_launch_variable(&parent, &mut values, "ComSpec");
        }

        insert_path(&mut values, "GIT_DIR", git_directory);
        insert_path(&mut values, "GIT_COMMON_DIR", git_directory);
        insert_path(&mut values, "GIT_INDEX_FILE", &git_directory.join("index"));
        insert_path(&mut values, "GIT_OBJECT_DIRECTORY", &git_directory.join("objects"));
        if let Some(work_tree) = work_tree {
            insert_path(&mut values, "GIT_WORK_TREE", work_tree);
        }
        insert_path(&mut values, "GIT_CONFIG_GLOBAL", global_config);
        insert_path(&mut values, "GIT_CONFIG_SYSTEM", global_config);
        insert_path(&mut values, "TMPDIR", process_temp);
        insert_path(&mut values, "TMP", process_temp);
        insert_path(&mut values, "TEMP", process_temp);

        insert(&mut values, "GIT_CONFIG_NOSYSTEM", "1");
        insert(&mut values, "GIT_CONFIG_COUNT", "0");
        insert(&mut values, "GIT_ATTR_NOSYSTEM", "1");
        insert(&mut values, "GIT_TERMINAL_PROMPT", "0");
        insert(&mut values, "GIT_AUTHOR_NAME", FIXED_NAME);
        insert(&mut values, "GIT_AUTHOR_EMAIL", FIXED_EMAIL);
        insert(&mut values, "GIT_COMMITTER_NAME", FIXED_NAME);
        insert(&mut values, "GIT_COMMITTER_EMAIL", FIXED_EMAIL);
        insert(&mut values, "GIT_AUTHOR_DATE", FIXED_DATE);
        insert(&mut values, "GIT_COMMITTER_DATE", FIXED_DATE);
        insert(&mut values, "LC_ALL", "C");
        insert(&mut values, "LANG", "C");
        insert(&mut values, "TZ", "UTC");
        Self { values }
    }

    pub(super) fn apply(&self, command: &mut Command) {
        command.env_clear();
        command.envs(&self.values);
    }
}

fn copy_launch_variable(
    parent: &BTreeMap<OsString, OsString>,
    values: &mut BTreeMap<OsString, OsString>,
    key: &str,
) {
    if let Some(value) = parent_value(parent, key) {
        values.insert(OsString::from(key), value.clone());
    }
}

#[cfg(not(windows))]
fn parent_value<'a>(parent: &'a BTreeMap<OsString, OsString>, key: &str) -> Option<&'a OsString> {
    parent.get(OsStr::new(key))
}

#[cfg(windows)]
fn parent_value<'a>(parent: &'a BTreeMap<OsString, OsString>, key: &str) -> Option<&'a OsString> {
    parent.iter().find_map(|(candidate, value)| {
        candidate.to_string_lossy().eq_ignore_ascii_case(key).then_some(value)
    })
}

fn insert(values: &mut BTreeMap<OsString, OsString>, key: &str, value: &str) {
    values.insert(OsString::from(key), OsString::from(value));
}

fn insert_path(values: &mut BTreeMap<OsString, OsString>, key: &str, value: &Path) {
    values.insert(OsString::from(key), value.as_os_str().to_owned());
}

#[cfg(test)]
mod tests {
    use super::{FIXED_EMAIL, FIXED_NAME, GitCommandContext, isolated_git_command};
    use std::collections::BTreeMap;
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    #[test]
    fn command_selects_no_hostile_parent_git_environment() {
        let hostile = [
            ("PATH", "/controlled/bin"),
            ("GIT_DIR", "/attacker/repository"),
            ("GIT_WORK_TREE", "/attacker/worktree"),
            ("GIT_COMMON_DIR", "/attacker/common"),
            ("GIT_INDEX_FILE", "/attacker/index"),
            ("GIT_OBJECT_DIRECTORY", "/attacker/objects"),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", "/attacker/alternate"),
            ("GIT_CONFIG_GLOBAL", "/attacker/config"),
            ("GIT_CONFIG_PARAMETERS", "'user.name=Attacker'"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_AUTHOR_NAME", "Attacker"),
            ("GIT_AUTHOR_EMAIL", "attacker@example.invalid"),
            ("GIT_COMMITTER_NAME", "Attacker"),
            ("GIT_COMMITTER_EMAIL", "attacker@example.invalid"),
            ("HOME", "/attacker/home"),
        ]
        .into_iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)));
        let command = isolated_git_command(
            &GitCommandContext {
                git_program: OsStr::new("inspect-only-git"),
                repository_root: Path::new("/owned/repository"),
                hooks_root: Path::new("/owned/disabled-hooks"),
                global_config: Path::new("/owned/isolated-gitconfig"),
                process_temp: Path::new("/owned/process-temp"),
                bare: false,
            },
            ["status"],
            hostile,
        );
        let explicit: BTreeMap<_, _> = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
            .collect();
        assert_eq!(
            explicit.get(OsStr::new("GIT_DIR")),
            Some(&OsString::from("/owned/repository/.git"))
        );
        assert_eq!(
            explicit.get(OsStr::new("GIT_WORK_TREE")),
            Some(&OsString::from("/owned/repository"))
        );
        assert_eq!(explicit.get(OsStr::new("GIT_AUTHOR_NAME")), Some(&OsString::from(FIXED_NAME)));
        assert_eq!(
            explicit.get(OsStr::new("GIT_AUTHOR_EMAIL")),
            Some(&OsString::from(FIXED_EMAIL))
        );
        assert_eq!(
            explicit.get(OsStr::new("GIT_COMMITTER_NAME")),
            Some(&OsString::from(FIXED_NAME))
        );
        assert_eq!(
            explicit.get(OsStr::new("GIT_COMMITTER_EMAIL")),
            Some(&OsString::from(FIXED_EMAIL))
        );
        assert!(!explicit.contains_key(OsStr::new("HOME")));
        assert!(!explicit.contains_key(OsStr::new("GIT_CONFIG_PARAMETERS")));
        assert!(!explicit.contains_key(OsStr::new("GIT_ALTERNATE_OBJECT_DIRECTORIES")));
    }

    #[cfg(windows)]
    #[test]
    fn command_canonicalizes_mixed_case_windows_launch_variables() {
        let parent = [
            (OsString::from("Path"), OsString::from(r"C:\controlled")),
            (OsString::from("pAtHeXt"), OsString::from(".EXE;.CMD")),
            (OsString::from("systemroot"), OsString::from(r"C:\Windows")),
            (OsString::from("COMSPEC"), OsString::from(r"C:\cmd.exe")),
        ];
        let command = isolated_git_command(
            &GitCommandContext {
                git_program: OsStr::new("inspect-only-git"),
                repository_root: Path::new(r"C:\owned\repository"),
                hooks_root: Path::new(r"C:\owned\disabled-hooks"),
                global_config: Path::new(r"C:\owned\isolated-gitconfig"),
                process_temp: Path::new(r"C:\owned\process-temp"),
                bare: false,
            },
            ["status"],
            parent,
        );
        let explicit: BTreeMap<_, _> = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
            .collect();

        assert_eq!(explicit.get(OsStr::new("PATH")), Some(&OsString::from(r"C:\controlled")));
        assert_eq!(explicit.get(OsStr::new("PATHEXT")), Some(&OsString::from(".EXE;.CMD")));
        assert_eq!(explicit.get(OsStr::new("SystemRoot")), Some(&OsString::from(r"C:\Windows")));
        assert_eq!(explicit.get(OsStr::new("ComSpec")), Some(&OsString::from(r"C:\cmd.exe")));
    }
}
