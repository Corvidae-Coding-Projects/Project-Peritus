//! Keep Git HEAD locks alive from preflight through source and index restoration.

use super::super::transaction::{Journal, Kind, Lock, own_directory};
use super::{failure, git, nested_head, recovery, text};
use crate::ProductRunnerError;
use std::{
    fs,
    io::{BufRead as _, BufReader, Read as _, Seek as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
};

pub(super) enum Change {
    Reference(Transaction),
    Symbolic(Symbolic),
}

impl Change {
    pub(super) fn prepare(
        root: &Path,
        baseline: Option<&str>,
        current: Option<&str>,
        unborn: &str,
        journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        if let Some(commit) = baseline {
            let expected = current.map_or_else(|| "0".repeat(commit.len()), str::to_owned);
            return Transaction::prepare(
                root,
                &format!("option no-deref\nupdate HEAD {commit} {expected}\n"),
            )
            .map(Self::Reference);
        }
        let output = super::super::repository_command(root)?
            .args(["config", "--get", "extensions.refStorage"])
            .output()
            .map_err(failure)?;
        if output.status.success() && text(output.stdout)? == "reftable" {
            let expected =
                current.ok_or_else(|| failure("unborn restore has no current commit"))?;
            return Transaction::prepare(
                root,
                &format!("symref-update HEAD {unborn} oid {expected}\n"),
            )
            .map(Self::Reference);
        }
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(failure(String::from_utf8_lossy(&output.stderr)));
        }
        Symbolic::prepare(root, current, unborn, journal).map(Self::Symbolic)
    }

    pub(super) fn publish(self, journal: Option<&mut Journal>) -> Result<(), ProductRunnerError> {
        match self {
            Self::Reference(transaction) => transaction.publish(),
            Self::Symbolic(symbolic) => symbolic.publish(journal),
        }
    }
}

pub(super) struct Transaction {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    errors: fs::File,
}

impl Transaction {
    fn prepare(root: &Path, update: &str) -> Result<Self, ProductRunnerError> {
        let errors = tempfile::tempfile().map_err(failure)?;
        let mut child = super::super::repository_command(root)?
            // Internal retention/restore refs must not invoke a user's reference hooks.
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.fsync=all",
                "update-ref",
                "--stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(errors.try_clone().map_err(failure)?)
            .spawn()
            .map_err(failure)?;
        let input = child.stdin.take().ok_or_else(|| failure("missing Git transaction input"))?;
        let output =
            child.stdout.take().ok_or_else(|| failure("missing Git transaction output"))?;
        let mut transaction =
            Self { child, input: Some(input), output: BufReader::new(output), errors };
        transaction.exchange("start\n", "start: ok")?;
        transaction.exchange(&format!("{update}prepare\n"), "prepare: ok")?;
        Ok(transaction)
    }

    fn exchange(&mut self, command: &str, expected: &str) -> Result<(), ProductRunnerError> {
        let input =
            self.input.as_mut().ok_or_else(|| failure("Git transaction input is closed"))?;
        input.write_all(command.as_bytes()).and_then(|()| input.flush()).map_err(failure)?;
        let mut response = String::new();
        self.output.read_line(&mut response).map_err(failure)?;
        if response.trim() != expected {
            self.errors.rewind().map_err(failure)?;
            let mut detail = String::new();
            std::io::Read::by_ref(&mut self.errors)
                .take(8192)
                .read_to_string(&mut detail)
                .map_err(failure)?;
            return Err(failure(format!(
                "Git HEAD transaction was not prepared: {response} {detail}"
            )));
        }
        Ok(())
    }

    fn publish(mut self) -> Result<(), ProductRunnerError> {
        self.exchange("commit\n", "commit: ok")?;
        self.input = None;
        let status = self.child.wait().map_err(failure)?;
        if !status.success() {
            return Err(failure("Git HEAD transaction did not commit"));
        }
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // EOF aborts an uncommitted Git transaction and releases its own locks.
        self.input = None;
        let _ = self.child.wait();
    }
}

pub(super) struct Symbolic {
    head: PathBuf,
    lock: Option<Lock>,
    directory: tempfile::TempDir,
}

impl Symbolic {
    fn prepare(
        root: &Path,
        current: Option<&str>,
        unborn: &str,
        mut journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        let head = PathBuf::from(text(git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "HEAD"],
            None,
        )?)?);
        if !fs::symlink_metadata(&head).map_err(failure)?.is_file() {
            return Err(failure("HEAD is not an ordinary Git file; no restore started"));
        }
        let lock_path = head.with_file_name("HEAD.lock");
        let lock = Lock::create(&lock_path, journal.as_deref_mut())?;
        let parent = head.parent().ok_or_else(|| failure("HEAD has no parent"))?;
        let mut directory =
            tempfile::Builder::new().prefix("peritus-head-").tempdir_in(parent).map_err(failure)?;
        own_directory(directory.path(), Kind::Head, journal.as_deref_mut())?;
        directory.disable_cleanup(journal.is_some());
        let symbolic = Self { head, lock: Some(lock), directory };
        if nested_head(root)?.as_deref() != current {
            return Err(failure("nested HEAD changed during discard preparation"));
        }
        let exists = super::super::repository_command(root)?
            .args(["show-ref", "--verify", "--quiet", unborn])
            .status()
            .map_err(failure)?;
        if exists.code() != Some(1) {
            return Err(failure("restored unborn branch name is unavailable"));
        }
        let permissions = fs::metadata(&symbolic.head).map_err(failure)?.permissions();
        let mut payload =
            fs::File::create(symbolic.directory.path().join("HEAD")).map_err(failure)?;
        // A symbolic HEAD can point at a fresh, absent ref without creating or deleting
        // a branch. The prior commit and symbolic name are retained in head.json.
        writeln!(payload, "ref: {unborn}").map_err(failure)?;
        payload.set_permissions(permissions).map_err(failure)?;
        payload.sync_all().map_err(failure)?;
        recovery::sync_directory(symbolic.directory.path())?;
        if let Some(journal) = journal {
            journal.seal_directory(symbolic.directory.path())?;
        }
        Ok(symbolic)
    }

    fn publish(mut self, journal: Option<&mut Journal>) -> Result<(), ProductRunnerError> {
        super::super::restore::publish_file(&self.directory.path().join("HEAD"), &self.head)?;
        recovery::sync_directory(self.head.parent().ok_or_else(|| failure("HEAD has no parent"))?)?;
        self.lock.take().ok_or_else(|| failure("HEAD lock is closed"))?.release()?;
        if let Some(journal) = journal {
            journal.cleanup_directory(self.directory.path())?;
        }
        Ok(())
    }
}
