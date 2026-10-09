//! Bounded structured commit history from one immutable start commit.

use std::ffi::OsString;

use peritus_types::Sha256Digest;

use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass,
    RegisteredWorktree, command::CommandAccess,
};

/// Maximum commits returned by one history observation.
pub const MAX_HISTORY_COMMITS: u16 = 1_024;

/// One structured commit observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitObservation {
    commit: CommitId,
    parents: Vec<CommitId>,
    timestamp_seconds: u64,
    subject: String,
}

impl CommitObservation {
    /// Returns the exact commit identity.
    #[must_use]
    pub const fn commit(&self) -> CommitId {
        self.commit
    }
    /// Returns ordered parent identities.
    #[must_use]
    pub fn parents(&self) -> &[CommitId] {
        &self.parents
    }
    /// Returns Git's committed Unix timestamp.
    #[must_use]
    pub const fn timestamp_seconds(&self) -> u64 {
        self.timestamp_seconds
    }
    /// Returns the bounded UTF-8 commit subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

/// Exact history start and count bound.
#[derive(Clone, Copy, Debug)]
pub struct HistoryRequest<'a> {
    worktree: &'a RegisteredWorktree,
    start: CommitId,
    maximum_commits: u16,
}

impl<'a> HistoryRequest<'a> {
    /// Creates a bounded immutable history request.
    ///
    /// # Errors
    /// Rejects zero or excessive commit counts.
    pub fn new(
        worktree: &'a RegisteredWorktree,
        start: CommitId,
        maximum_commits: u16,
    ) -> Result<Self, GitError> {
        if maximum_commits == 0 || maximum_commits > MAX_HISTORY_COMMITS {
            return Err(input_error("Git history count is zero or exceeds its hard maximum"));
        }
        Ok(Self { worktree, start, maximum_commits })
    }
}

/// Complete repository-bound history observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHistoryObservation {
    repository_digest: Sha256Digest,
    start: CommitId,
    commits: Vec<CommitObservation>,
    digest: Sha256Digest,
}

impl GitHistoryObservation {
    /// Returns the repository binding.
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }
    /// Returns the exact history start.
    #[must_use]
    pub const fn start(&self) -> CommitId {
        self.start
    }
    /// Returns commits in Git's deterministic newest-first order.
    #[must_use]
    pub fn commits(&self) -> &[CommitObservation] {
        &self.commits
    }
    /// Returns the canonical observation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

impl GitRepository {
    /// Observes bounded commit history through one fixed read-only Git command.
    ///
    /// # Errors
    /// Returns typed registration, command, object, bounds, UTF-8, or protocol failure.
    pub fn history(&self, request: HistoryRequest<'_>) -> Result<GitHistoryObservation, GitError> {
        self.validate_registration(request.worktree, Operation::History)?;
        let mut arguments = crate::repository::strings(&[
            "log",
            "-z",
            "--format=%H%x00%P%x00%at%x00%s",
            "--no-decorate",
        ]);
        arguments.push(OsString::from(format!("--max-count={}", request.maximum_commits)));
        arguments.push(OsString::from(request.start.to_string()));
        arguments.push(OsString::from("--"));
        let output = self.runner.checked(
            request.worktree.root(),
            Some(Self::worktree_location(request.worktree.root(), request.worktree.git_dir())),
            CommandAccess::Read,
            Operation::History,
            &arguments,
            None,
        )?;
        let commits =
            parse_history(&output.stdout, self.identity.object_format(), request.maximum_commits)?;
        let digest = history_digest(self.identity.digest(), request.start, &commits);
        Ok(GitHistoryObservation {
            repository_digest: self.identity.digest(),
            start: request.start,
            commits,
            digest,
        })
    }
}

fn parse_history(
    bytes: &[u8],
    format: crate::ObjectFormat,
    maximum: u16,
) -> Result<Vec<CommitObservation>, GitError> {
    let mut fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    if fields.len() % 4 != 0 || fields.len() / 4 > maximum as usize {
        return Err(protocol("Git history output is malformed or exceeds its bound"));
    }
    let mut commits = Vec::with_capacity(fields.len() / 4);
    for record in fields.chunks_exact(4) {
        let commit = parse_commit(record[0], format)?;
        let parent_text = std::str::from_utf8(record[1])
            .map_err(|_| protocol("Git history parents are not UTF-8"))?;
        let parents = if parent_text.is_empty() {
            Vec::new()
        } else {
            parent_text
                .split(' ')
                .map(|parent| {
                    ObjectId::parse(format, parent, Operation::History).map(CommitId::checked)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let timestamp_seconds = std::str::from_utf8(record[2])
            .map_err(|_| protocol("Git history timestamp is not UTF-8"))?
            .parse::<u64>()
            .map_err(|_| protocol("Git history timestamp is invalid"))?;
        let subject = std::str::from_utf8(record[3])
            .map_err(|_| protocol("Git history subject is not UTF-8"))?;
        if subject.len() > 4_096 || subject.bytes().any(|byte| byte == 0) {
            return Err(protocol("Git history subject exceeds its bound"));
        }
        commits.push(CommitObservation {
            commit,
            parents,
            timestamp_seconds,
            subject: subject.to_owned(),
        });
    }
    Ok(commits)
}

fn parse_commit(bytes: &[u8], format: crate::ObjectFormat) -> Result<CommitId, GitError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| protocol("Git history commit identity is not UTF-8"))?;
    ObjectId::parse(format, text, Operation::History).map(CommitId::checked)
}

fn history_digest(
    repository: Sha256Digest,
    start: CommitId,
    commits: &[CommitObservation],
) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-HISTORY-V1\0".to_vec();
    bytes.extend_from_slice(repository.as_bytes());
    bytes.extend_from_slice(start.object_id().as_bytes());
    bytes.extend_from_slice(&(commits.len() as u64).to_be_bytes());
    for commit in commits {
        bytes.extend_from_slice(commit.commit.object_id().as_bytes());
        bytes.extend_from_slice(&(commit.parents.len() as u64).to_be_bytes());
        for parent in &commit.parents {
            bytes.extend_from_slice(parent.object_id().as_bytes());
        }
        bytes.extend_from_slice(&commit.timestamp_seconds.to_be_bytes());
        crate::status::put_bytes(&mut bytes, commit.subject.as_bytes());
    }
    peritus_codec::sha256(&bytes)
}

fn input_error(detail: &'static str) -> GitError {
    GitError::new(
        ErrorKind::InvalidInput,
        Operation::History,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

fn protocol(detail: &'static str) -> GitError {
    GitError::new(ErrorKind::GitProtocol, Operation::History, RecoveryClass::Reobserve, detail)
}
