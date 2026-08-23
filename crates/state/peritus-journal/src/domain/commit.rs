//! Shared post-commit state observation path.

use crate::{
    AppendRequest, CommittedBatch, DurableStateRecord, JournalError, JournalErrorKind,
    SqliteJournal, StateInstall,
};

pub(super) fn commit_state(
    journal: &mut SqliteJournal,
    append: AppendRequest,
    domain: &[u8],
    install: StateInstall,
) -> Result<(CommittedBatch, DurableStateRecord), JournalError> {
    let namespace = install.namespace();
    let key = install.key().to_vec();
    let expected_revision = install.expected_revision();
    let revision = install.revision();
    let digest = install.digest();
    let plan = append.bind_domain_state(domain, vec![install])?.plan()?;
    let committed = journal.append(plan)?;
    let observed = journal.state_record_revision(namespace, &key, revision)?.ok_or_else(|| {
        JournalError::new(
            JournalErrorKind::CorruptJournal,
            "observe committed domain state",
            "committed state row is missing",
        )
    })?;
    if !crate::verified::committed_state_successor(expected_revision, revision, observed.revision())
        || observed.digest() != digest
        || observed.producing_position() != committed.last_position()
    {
        return Err(JournalError::new(
            JournalErrorKind::CorruptJournal,
            "observe committed domain state",
            "state row does not match the exact committed transition",
        ));
    }
    Ok((committed, observed))
}

pub(super) fn successor(expected: Option<u64>) -> Result<u64, JournalError> {
    expected.map_or(Some(1), |value| value.checked_add(1)).ok_or_else(|| {
        JournalError::new(
            JournalErrorKind::SequenceOverflow,
            "plan domain state",
            "domain state revision exhausted",
        )
    })
}
