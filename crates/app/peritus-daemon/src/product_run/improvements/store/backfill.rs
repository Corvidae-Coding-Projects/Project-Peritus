//! Durable one-time ownership of terminal-run suggestion collection.

use super::{Error, Store, chunks, digest, problem, schema, text};
use peritus_types::{RunId, WorkspaceId};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

impl Store {
    /// Records one recovered terminal run exactly once, including runs that do not suggest a
    /// candidate. The source marker and any candidate/evidence rows commit together.
    pub(super) fn backfill_run(
        &mut self,
        workspace: WorkspaceId,
        run: RunId,
        suggestion: Option<(&str, &str)>,
    ) -> Result<(), Error> {
        let fingerprint = match suggestion {
            Some((proposal, _)) => digest(&[
                b"peritus.improvement.backfill.v1",
                workspace.as_bytes(),
                run.as_bytes(),
                proposal.as_bytes(),
            ]),
            None => digest(&[
                b"peritus.improvement.backfill.v1",
                workspace.as_bytes(),
                run.as_bytes(),
            ]),
        };
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(problem)?;
        let retained: Option<[u8; 32]> = transaction
            .query_row(
                "SELECT source_digest FROM improvement_backfill WHERE workspace=?1 AND run=?2",
                params![workspace.as_bytes(), run.as_bytes()],
                |row| row.get(0),
            )
            .optional()
            .map_err(problem)?;
        if retained == Some(fingerprint) {
            // A restart over the same terminal observation has already been reconciled. The
            // formatted evidence body may contain the collecting package version, so the stable
            // ownership identity intentionally binds the run and suggestion class instead.
            transaction.commit().map_err(problem)?;
            return Ok(());
        }
        let candidate = suggestion
            .map(|(proposal, summary)| {
                text(proposal)?;
                text(summary)?;
                collect_candidate(&transaction, workspace, run, proposal, summary)
            })
            .transpose()?;
        // Waiting/recovery records can later resume under the same run identity. Advance the
        // durable source marker only after the new terminal suggestion class and its evidence have
        // committed in this transaction; already accepted evidence remains immutable by run key.
        transaction
            .execute(
                "INSERT INTO improvement_backfill(workspace,run,source_digest,candidate) VALUES (?1,?2,?3,?4)
                 ON CONFLICT(workspace,run) DO UPDATE SET source_digest=excluded.source_digest,candidate=excluded.candidate",
                params![workspace.as_bytes(), run.as_bytes(), fingerprint, candidate],
            )
            .map_err(problem)?;
        transaction.commit().map_err(problem)
    }
}

pub(super) fn collect_candidate(
    transaction: &Transaction<'_>,
    workspace: WorkspaceId,
    run: RunId,
    proposal: &str,
    summary: &str,
) -> Result<[u8; 32], Error> {
    let normalized = normalize(proposal);
    let id = digest(&[b"peritus.improvement.v1", workspace.as_bytes(), normalized.as_bytes()]);
    let proposal_digest = digest(&[b"peritus.improvement.proposal.v1", proposal.as_bytes()]);
    let observation = digest(&[
        b"peritus.improvement.observation.v1",
        run.as_bytes(),
        summary.as_bytes(),
    ]);
    let created = transaction
        .execute(
            "INSERT INTO improvement_candidates(workspace,id,proposal,proposal_digest,dismissed) VALUES (?1,?2,?3,?4,0) ON CONFLICT(workspace,id) DO NOTHING",
            params![workspace.as_bytes(), id, proposal, proposal_digest],
        )
        .map_err(problem)?;
    if created != 0 {
        chunks::write(
            transaction,
            workspace.as_bytes(),
            &id,
            &[0; 16],
            proposal_digest,
            proposal,
        )?;
    } else {
        let retained: ([u8; 32], String) = transaction
            .query_row(
                "SELECT proposal_digest,proposal FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(problem)?;
        if retained.0 != digest(&[b"peritus.improvement.proposal.v1", retained.1.as_bytes()])
            || id
                != digest(&[
                    b"peritus.improvement.v1",
                    workspace.as_bytes(),
                    normalize(&retained.1).as_bytes(),
                ])
        {
            return Err(problem("retained improvement candidate identity is corrupt"));
        }
    }
    // The indexed source-run key replaces a scan of every prior observation. An evaluation
    // reservation freezes evidence atomically with concurrent collection.
    let changed = transaction
        .execute(
            "INSERT INTO improvement_evidence(workspace,candidate,run,digest,summary) SELECT ?1,?2,?3,?4,?5 WHERE EXISTS (SELECT 1 FROM improvement_candidates WHERE workspace=?1 AND id=?2 AND evaluation IS NULL) ON CONFLICT(workspace,candidate,run) DO NOTHING",
            params![workspace.as_bytes(), id, run.as_bytes(), observation, summary],
        )
        .map_err(problem)?;
    if changed != 0 {
        transaction
            .execute(
                "UPDATE improvement_candidates SET evidence_count=evidence_count+1 WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
            )
            .map_err(problem)?;
        chunks::write(
            transaction,
            workspace.as_bytes(),
            &id,
            run.as_bytes(),
            observation,
            summary,
        )?;
        schema::advance(transaction, workspace.as_bytes())?;
    }
    Ok(id)
}

pub(super) fn normalize(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for word in value.split_whitespace() {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.extend(word.chars().flat_map(char::to_lowercase));
    }
    normalized
}
