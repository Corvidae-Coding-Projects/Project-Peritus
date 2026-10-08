//! Indexed keyset reads with independently addressed immutable UTF-8 source slices.

use super::{Error, Evaluation, Store, chunks, problem};
use peritus_app_protocol::{
    IMPROVEMENT_PAGE_ITEMS, ImprovementCandidateSummary,
    ImprovementEvidencePage, ImprovementEvidenceSummary, ImprovementPage, ImprovementPageCursor,
    ImprovementTextPage, ImprovementTextQuery, ImprovementTextReference,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use rusqlite::{OptionalExtension, Transaction, params};

impl Store {
    pub(super) fn page(&mut self, workspace: WorkspaceId, after: Option<ImprovementPageCursor>) -> Result<ImprovementPage, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        let current_revision = revision(&transaction, workspace)?;
        let (revision, highwater, sequence) = match after {
            Some(value) => cursor(workspace, None, current_revision, value)?,
            None => (
                current_revision,
                transaction
                    .query_row(
                        "SELECT coalesce(max(sequence),0) FROM improvement_candidates WHERE workspace=?1",
                        [workspace.as_bytes()],
                        |row| row.get(0),
                    )
                    .map_err(problem)?,
                0,
            ),
        };
        let mut statement = transaction.prepare(
            "SELECT c.sequence,c.id,c.proposal_digest,length(CAST(c.proposal AS BLOB)),c.evaluation,c.dismissed,c.evidence_count
             FROM improvement_candidates c WHERE c.workspace=?1
                AND c.sequence<=?2 AND (?3=0 OR c.sequence<?3)
             ORDER BY c.sequence DESC LIMIT ?4",
        ).map_err(problem)?;
        let mut rows = statement.query(params![workspace.as_bytes(), highwater, sequence, (IMPROVEMENT_PAGE_ITEMS + 1) as u64])
            .map_err(problem)?;
        let mut candidates = Vec::new();
        let mut last = None;
        let mut more = false;
        while let Some(row) = rows.next().map_err(problem)? {
            if candidates.len() == IMPROVEMENT_PAGE_ITEMS { more = true; break; }
            let sequence: u64 = row.get(0).map_err(problem)?;
            let id: [u8; 32] = row.get(1).map_err(problem)?;
            let raw: Option<String> = row.get(4).map_err(problem)?;
            let evaluation = raw.map(|value| {
                serde_json::from_str::<Evaluation>(&value).map_err(problem)?.project(workspace.into_bytes(), id)
            }).transpose()?;
            let dismissed: bool = row.get(5).map_err(problem)?;
            let proposal = ImprovementTextReference::new(
                Sha256Digest::new(row.get(2).map_err(problem)?),
                row.get(3).map_err(problem)?,
            )
            .map_err(problem)?;
            candidates.push(ImprovementCandidateSummary::new(
                Sha256Digest::new(id),
                proposal,
                row.get(6).map_err(problem)?, evaluation, dismissed,
            ));
            last = Some(
                ImprovementPageCursor::snapshot(
                    workspace,
                    None,
                    revision,
                    highwater,
                    sequence,
                )
                    .map_err(problem)?,
            );
        }
        ImprovementPage::new(workspace, revision, candidates, if more { last } else { None }).map_err(problem)
    }

    pub(super) fn evidence_page(&mut self, workspace: WorkspaceId, candidate: Sha256Digest, expected_revision: u64, after: Option<ImprovementPageCursor>) -> Result<ImprovementEvidencePage, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        let current_revision = revision(&transaction, workspace)?;
        if expected_revision == 0 || expected_revision > current_revision {
            return Err(stale());
        }
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM improvement_candidates WHERE workspace=?1 AND id=?2)",
            params![workspace.as_bytes(), candidate.as_bytes()], |r| r.get(0),
        ).map_err(problem)?;
        if !exists { return Err(Error::NotFound); }
        let (revision, highwater, sequence) = match after {
            Some(value) => {
                let selected = cursor(workspace, Some(candidate), current_revision, value)?;
                if selected.0 != expected_revision {
                    return Err(stale());
                }
                selected
            }
            None => (
                expected_revision,
                transaction
                    .query_row(
                        "SELECT coalesce(max(sequence),0) FROM improvement_evidence WHERE workspace=?1 AND candidate=?2",
                        params![workspace.as_bytes(), candidate.as_bytes()],
                        |row| row.get(0),
                    )
                    .map_err(problem)?,
                0,
            ),
        };
        let mut statement = transaction.prepare(
            "SELECT sequence,run,digest,length(CAST(summary AS BLOB)) FROM improvement_evidence
             WHERE workspace=?1 AND candidate=?2 AND sequence<=?3 AND (?4=0 OR sequence>?4)
             ORDER BY sequence LIMIT ?5",
        ).map_err(problem)?;
        let mut rows = statement.query(params![workspace.as_bytes(), candidate.as_bytes(), highwater, sequence, (IMPROVEMENT_PAGE_ITEMS + 1) as u64])
            .map_err(problem)?;
        let mut evidence = Vec::new();
        let mut last = None;
        let mut more = false;
        while let Some(row) = rows.next().map_err(problem)? {
            if evidence.len() == IMPROVEMENT_PAGE_ITEMS { more = true; break; }
            let sequence: u64 = row.get(0).map_err(problem)?;
            let summary = ImprovementTextReference::new(
                Sha256Digest::new(row.get(2).map_err(problem)?),
                row.get(3).map_err(problem)?,
            )
            .map_err(problem)?;
            evidence.push(ImprovementEvidenceSummary::new(
                RunId::new(row.get(1).map_err(problem)?).map_err(problem)?,
                summary,
            ));
            last = Some(
                ImprovementPageCursor::snapshot(
                    workspace,
                    Some(candidate),
                    revision,
                    highwater,
                    sequence,
                )
                .map_err(problem)?,
            );
        }
        ImprovementEvidencePage::new(workspace, candidate, revision, evidence, if more { last } else { None }).map_err(problem)
    }

    pub(super) fn candidate_page(
        &mut self,
        workspace: WorkspaceId,
        candidate: Sha256Digest,
    ) -> Result<ImprovementPage, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        let revision = revision(&transaction, workspace)?;
        let value: Option<([u8; 32], u64, Option<String>, bool, u64)> = transaction
            .query_row(
                "SELECT proposal_digest,length(CAST(proposal AS BLOB)),evaluation,dismissed,evidence_count
                 FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), candidate.as_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(problem)?;
        let (proposal_digest, bytes, raw, dismissed, evidence_count) =
            value.ok_or(Error::NotFound)?;
        let evaluation = raw
            .map(|value| {
                serde_json::from_str::<Evaluation>(&value)
                    .map_err(problem)?
                    .project(workspace.into_bytes(), candidate.into_bytes())
            })
            .transpose()?;
        let proposal = ImprovementTextReference::new(
            Sha256Digest::new(proposal_digest),
            bytes,
        )
        .map_err(problem)?;
        let summary = ImprovementCandidateSummary::new(
            candidate,
            proposal,
            evidence_count,
            evaluation,
            dismissed,
        );
        ImprovementPage::new(workspace, revision, vec![summary], None).map_err(problem)
    }

    pub(super) fn text_page(&mut self, query: ImprovementTextQuery) -> Result<ImprovementTextPage, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        i64::try_from(query.offset()).map_err(|_| Error::InvalidState)?;
        let value: Option<([u8; 32], u64)> = if let Some(run) = query.run() {
            transaction.query_row(
                "SELECT digest,length(CAST(summary AS BLOB)) FROM improvement_evidence WHERE workspace=?1 AND candidate=?2 AND run=?3",
                params![query.workspace().as_bytes(), query.candidate().as_bytes(), run.as_bytes()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).optional().map_err(problem)?
        } else {
            transaction.query_row(
                "SELECT proposal_digest,length(CAST(proposal AS BLOB)) FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![query.workspace().as_bytes(), query.candidate().as_bytes()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).optional().map_err(problem)?
        };
        let (digest, length) = value.ok_or(Error::NotFound)?;
        if digest != query.source().digest().into_bytes() || length != query.source().bytes() || query.offset() > length {
            return Err(Error::InvalidState);
        }
        chunks::read(&transaction, query)
    }
}

fn revision(transaction: &Transaction<'_>, workspace: WorkspaceId) -> Result<u64, Error> {
    Ok(transaction.query_row("SELECT revision FROM improvement_revisions WHERE workspace=?1", [workspace.as_bytes()], |r| r.get(0))
        .optional().map_err(problem)?.unwrap_or(0))
}

fn cursor(
    workspace: WorkspaceId,
    candidate: Option<Sha256Digest>,
    current_revision: u64,
    value: ImprovementPageCursor,
) -> Result<(u64, u64, u64), Error> {
    if value.workspace() != workspace
        || value.candidate() != candidate
        || value.revision() > current_revision
        || value.highwater_sequence() == 0
        || value.highwater_sequence() > i64::MAX as u64
        || value.sequence() == 0
        || value.sequence() > value.highwater_sequence()
        || value.dismissed()
    {
        return Err(stale());
    }
    Ok((value.revision(), value.highwater_sequence(), value.sequence()))
}

fn stale() -> Error {
    Error::invalid_data(
        "read improvement history",
        "The retained history changed. Refresh the first candidate page",
    )
}
