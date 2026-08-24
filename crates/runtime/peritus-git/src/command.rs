//! Narrow shell-free Git subprocess boundary.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use crate::{ErrorKind, GitError, Operation, RecoveryClass};

pub const DEFAULT_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAccess {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug)]
pub struct RepositoryLocation<'a> {
    pub(crate) git_dir: &'a Path,
    pub(crate) work_tree: Option<&'a Path>,
}

#[derive(Debug)]
pub struct CommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct GitRunner {
    program: OsString,
    output_limit: usize,
}

impl GitRunner {
    pub(crate) const fn new(program: OsString, output_limit: usize) -> Self {
        Self { program, output_limit }
    }

    pub(crate) fn checked(
        &self,
        cwd: &Path,
        repository: Option<RepositoryLocation<'_>>,
        access: CommandAccess,
        operation: Operation,
        arguments: &[OsString],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, GitError> {
        let output = self.observe(cwd, repository, access, operation, arguments, stdin)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(GitError::command(operation, output.status.code(), &output.stderr))
        }
    }

    pub(crate) fn observe(
        &self,
        cwd: &Path,
        repository: Option<RepositoryLocation<'_>>,
        access: CommandAccess,
        operation: Operation,
        arguments: &[OsString],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, GitError> {
        let mut command = Command::new(&self.program);
        command.current_dir(cwd);
        command.arg("--no-pager").arg("--literal-pathspecs");
        if access == CommandAccess::Read {
            command.arg("--no-optional-locks");
        }
        command
            .arg("-c")
            .arg("credential.helper=")
            .arg("-c")
            .arg("core.askPass=")
            .arg("-c")
            .arg("commit.gpgSign=false")
            .arg("-c")
            .arg("tag.gpgSign=false")
            .arg("-c")
            .arg("core.fsmonitor=false")
            .arg("-c")
            .arg("core.untrackedCache=false")
            .arg("-c")
            .arg("core.autocrlf=false")
            .arg("-c")
            .arg("core.hooksPath=/dev/null");
        if let Some(repository) = repository {
            command.arg(git_path_argument("--git-dir=", repository.git_dir));
            if let Some(work_tree) = repository.work_tree {
                command.arg(git_path_argument("--work-tree=", work_tree));
            }
        }
        command.args(arguments);
        apply_environment(&mut command);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        let mut child =
            command.spawn().map_err(|source| GitError::unavailable(operation, source))?;
        let stdout =
            child.stdout.take().ok_or_else(|| protocol(operation, "Git stdout pipe missing"))?;
        let stderr =
            child.stderr.take().ok_or_else(|| protocol(operation, "Git stderr pipe missing"))?;
        let stdout_limit = self.output_limit;
        let stderr_limit = self.output_limit.min(crate::error::MAX_ERROR_STDERR_BYTES * 4);
        let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, stderr_limit));
        let stdin_writer = match (stdin, child.stdin.take()) {
            (Some(bytes), Some(mut pipe)) => {
                let bytes = bytes.to_vec();
                Some(thread::spawn(move || pipe.write_all(&bytes)))
            }
            (Some(_), None) => return Err(protocol(operation, "Git stdin pipe missing")),
            (None, _) => None,
        };
        let status = child.wait().map_err(|source| {
            GitError::io(operation, RecoveryClass::Retry, "wait for Git", source)
        })?;
        if let Some(writer) = stdin_writer {
            join_io(writer, operation, "write Git stdin")?;
        }
        let (stdout, stdout_overflow) = join_reader(stdout_reader, operation, "read Git stdout")?;
        let (stderr, stderr_overflow) = join_reader(stderr_reader, operation, "read Git stderr")?;
        if stdout_overflow || stderr_overflow {
            return Err(protocol(operation, "Git output exceeded the configured byte limit"));
        }
        Ok(CommandOutput { status, stdout, stderr })
    }
}

fn git_path_argument(prefix: &str, path: &Path) -> OsString {
    let mut argument = OsString::from(prefix);
    argument.push(git_path(path));
    argument
}

pub fn git_path(path: &Path) -> OsString {
    #[cfg(not(windows))]
    {
        path.as_os_str().to_owned()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

        const VERBATIM_PREFIX: &[u16] = &[92, 92, 63, 92];
        const VERBATIM_UNC_PREFIX: &[u16] = &[92, 92, 63, 92, 85, 78, 67, 92];
        let encoded: Vec<_> = path.as_os_str().encode_wide().collect();
        match (encoded.strip_prefix(VERBATIM_UNC_PREFIX), encoded.strip_prefix(VERBATIM_PREFIX)) {
            (Some(remainder), _) => {
                let mut conventional = vec![92_u16, 92];
                conventional.extend_from_slice(remainder);
                OsString::from_wide(&conventional)
            }
            (None, Some(remainder)) => OsString::from_wide(remainder),
            (None, None) => path.as_os_str().to_owned(),
        }
    }
}

fn apply_environment(command: &mut Command) {
    let parent: BTreeMap<_, _> = std::env::vars_os().collect();
    command.env_clear();
    copy_parent(&parent, command, "PATH");
    #[cfg(windows)]
    {
        copy_parent(&parent, command, "PATHEXT");
        copy_parent(&parent, command, "SystemRoot");
        copy_parent(&parent, command, "ComSpec");
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1");
    command.env("GIT_CONFIG_GLOBAL", null_device());
    command.env("GIT_ATTR_NOSYSTEM", "1");
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_CONFIG_COUNT", "0");
    command.env("GIT_AUTHOR_NAME", "Project Peritus");
    command.env("GIT_AUTHOR_EMAIL", "peritus@example.invalid");
    command.env("GIT_COMMITTER_NAME", "Project Peritus");
    command.env("GIT_COMMITTER_EMAIL", "peritus@example.invalid");
    command.env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z");
    command.env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z");
    command.env("LC_ALL", "C");
    command.env("LANG", "C");
    command.env("TZ", "UTC");
}

fn copy_parent(parent: &BTreeMap<OsString, OsString>, command: &mut Command, key: &str) {
    if let Some(value) = parent.get(OsStr::new(key)) {
        command.env(key, value);
    }
}

#[cfg(not(windows))]
const fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(windows)]
const fn null_device() -> &'static str {
    "NUL"
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut overflow = false;
    let mut buffer = [0_u8; 8_192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        let retained = count.min(remaining);
        output.extend_from_slice(&buffer[..retained]);
        overflow |= retained != count;
    }
    Ok((output, overflow))
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
    operation: Operation,
    detail: &'static str,
) -> Result<(Vec<u8>, bool), GitError> {
    handle
        .join()
        .map_err(|_| protocol(operation, "Git pipe reader panicked"))?
        .map_err(|source| GitError::io(operation, RecoveryClass::Retry, detail, source))
}

fn join_io(
    handle: thread::JoinHandle<io::Result<()>>,
    operation: Operation,
    detail: &'static str,
) -> Result<(), GitError> {
    handle
        .join()
        .map_err(|_| protocol(operation, "Git stdin writer panicked"))?
        .map_err(|source| GitError::io(operation, RecoveryClass::Retry, detail, source))
}

pub fn one_line(output: &[u8], operation: Operation) -> Result<&str, GitError> {
    let text = std::str::from_utf8(output)
        .map_err(|_| protocol(operation, "Git returned non-UTF-8 scalar output"))?;
    let value = text.strip_suffix('\n').unwrap_or(text);
    if value.is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(protocol(operation, "Git returned a malformed scalar output"));
    }
    Ok(value)
}

pub fn protocol(operation: Operation, detail: &'static str) -> GitError {
    GitError::new(ErrorKind::GitProtocol, operation, RecoveryClass::Quarantine, detail)
}
