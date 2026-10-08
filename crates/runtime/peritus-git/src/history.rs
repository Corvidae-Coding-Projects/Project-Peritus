//! Bounded structured commit history from one immutable start commit.

use std::ffi::OsString;

use peritus_types::Sha256Digest;

use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass,
    RegisteredWorktree, command::CommandAccess,
};

/// Maximum commits returned by one history observation.
pub const MAX_HISTORY_COMMITS: u16 = 1_024;
/// Maximum commit parents returned for one history row.
pub const MAX_HISTORY_PARENTS: u32 = 32;

/// One structured commit observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitObservation {
    commit: CommitId,
    parents: Vec<CommitId>,
    parent_offset: u32,
    parent_count: u32,
    next_parent_offset: Option<u32>,
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
    /// Returns the offset represented by this parent page.
    #[must_use]
    pub const fn parent_offset(&self) -> u32 {
        self.parent_offset
    }
    /// Returns the complete ordered parent count.
    #[must_use]
    pub const fn parent_count(&self) -> u32 {
        self.parent_count
    }
    /// Returns the next parent offset when more parents remain.
    #[must_use]
    pub const fn next_parent_offset(&self) -> Option<u32> {
        self.next_parent_offset
    }
    /// Returns Git's committed Unix timestamp.
    #[must_use]
    pub const fn timestamp_seconds(&self) -> u64 {
        self.timestamp_seconds
    }
    /// Returns the complete UTF-8 commit subject for paged rendering.
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
    offset: u64,
    parent_offset: u32,
    subject_offset: u32,
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
        Ok(Self {
            worktree,
            start,
            maximum_commits,
            offset: 0,
            parent_offset: 0,
            subject_offset: 0,
        })
    }

    /// Creates a continuation page bound to the same immutable history start.
    ///
    /// # Errors
    /// Rejects zero or excessive commit counts.
    pub fn page(
        worktree: &'a RegisteredWorktree,
        start: CommitId,
        maximum_commits: u16,
        offset: u64,
    ) -> Result<Self, GitError> {
        let mut request = Self::new(worktree, start, maximum_commits)?;
        request.offset = offset;
        Ok(request)
    }

    /// Adds a parent-list page offset to this history page.
    #[must_use]
    pub const fn with_parent_offset(mut self, parent_offset: u32) -> Self {
        self.parent_offset = parent_offset;
        self
    }

    /// Adds a byte offset for the first commit subject in the page.
    #[must_use]
    pub const fn with_subject_offset(mut self, subject_offset: u32) -> Self {
        self.subject_offset = subject_offset;
        self
    }
}

/// Complete repository-bound history observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHistoryObservation {
    repository_digest: Sha256Digest,
    start: CommitId,
    commits: Vec<CommitObservation>,
    offset: u64,
    next_offset: Option<u64>,
    parent_offset: u32,
    subject_offset: u32,
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
    /// Returns the exact offset represented by this history page.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Returns commits in Git's deterministic newest-first order.
    #[must_use]
    pub fn commits(&self) -> &[CommitObservation] {
        &self.commits
    }
    /// Returns the exact next offset, bound to the same immutable history start.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }
    /// Returns the requested parent page offset.
    #[must_use]
    pub const fn parent_offset(&self) -> u32 {
        self.parent_offset
    }
    /// Returns the requested first-subject byte offset.
    #[must_use]
    pub const fn subject_offset(&self) -> u32 {
        self.subject_offset
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
            "--no-show-signature",
            "--no-notes",
            "--no-color",
            "--no-patch",
        ]);
        arguments.push(OsString::from(format!("--max-count={}", request.maximum_commits + 1)));
        arguments.push(OsString::from(format!("--skip={}", request.offset)));
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
        let mut commits = parse_history(
            &output.stdout,
            self.identity.object_format(),
            request.maximum_commits + 1,
        )?;
        let has_more = commits.len() > request.maximum_commits as usize;
        if has_more {
            commits.pop();
        }
        for (index, commit) in commits.iter_mut().enumerate() {
            let parent_count = u32::try_from(commit.parents.len())
                .map_err(|_| protocol("Git history parent count is not representable"))?;
            let start = usize::try_from(request.parent_offset)
                .map_err(|_| protocol("Git history parent offset is not representable"))?
                .min(commit.parents.len());
            let end = start.saturating_add(MAX_HISTORY_PARENTS as usize).min(commit.parents.len());
            let next_parent_offset =
                (end < commit.parents.len()).then(|| u32::try_from(end).ok()).flatten();
            commit.parents = commit.parents[start..end].to_vec();
            commit.parent_offset = u32::try_from(start)
                .map_err(|_| protocol("Git history parent offset is not representable"))?;
            commit.parent_count = parent_count;
            commit.next_parent_offset = next_parent_offset;
            if index == 0 {
                let subject_len = u32::try_from(commit.subject.len())
                    .map_err(|_| protocol("Git history subject length is not representable"))?;
                if request.subject_offset > subject_len {
                    return Err(input_error("Git history subject offset exceeds its byte length"));
                }
            }
        }
        let next_offset = has_more
            .then(|| request.offset.checked_add(u64::from(request.maximum_commits)))
            .flatten();
        let digest = history_digest(
            self.identity.digest(),
            request.start,
            request.offset,
            request.subject_offset,
            &commits,
        );
        Ok(GitHistoryObservation {
            repository_digest: self.identity.digest(),
            start: request.start,
            commits,
            offset: request.offset,
            next_offset,
            parent_offset: request.parent_offset,
            subject_offset: request.subject_offset,
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
        commits.push(CommitObservation {
            commit,
            parents,
            parent_offset: 0,
            parent_count: 0,
            next_parent_offset: None,
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
    offset: u64,
    subject_offset: u32,
    commits: &[CommitObservation],
) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-HISTORY-V2\0".to_vec();
    bytes.extend_from_slice(repository.as_bytes());
    bytes.extend_from_slice(start.object_id().as_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    bytes.extend_from_slice(&subject_offset.to_be_bytes());
    // The history digest binds the cursor that selected the first subject range.
    bytes.extend_from_slice(&(commits.len() as u64).to_be_bytes());
    for commit in commits {
        bytes.extend_from_slice(commit.commit.object_id().as_bytes());
        bytes.extend_from_slice(&(commit.parents.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&commit.parent_offset.to_be_bytes());
        bytes.extend_from_slice(&commit.parent_count.to_be_bytes());
        bytes.extend_from_slice(&commit.next_parent_offset.unwrap_or(u32::MAX).to_be_bytes());
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
