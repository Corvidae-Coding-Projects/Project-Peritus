//! Atomic normalization of retained inbox state. Failed migration leaves the old schema intact.

use super::{Candidate, Error, chunks, digest, problem};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

pub(super) fn initialize(connection: &mut Connection) -> Result<(), Error> {
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
        .map_err(problem)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(problem)?;
    let version: u32 = transaction.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(problem)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS improvement_candidates (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace BLOB NOT NULL, id BLOB NOT NULL, proposal TEXT NOT NULL, proposal_digest BLOB NOT NULL,
            evidence_count INTEGER NOT NULL DEFAULT 0,
            evaluation TEXT, evaluation_run BLOB, dismissed INTEGER NOT NULL, UNIQUE(workspace,id));
         CREATE INDEX IF NOT EXISTS improvement_candidates_page ON improvement_candidates(workspace,dismissed,sequence DESC);
         CREATE TABLE IF NOT EXISTS improvement_evidence (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace BLOB NOT NULL, candidate BLOB NOT NULL, run BLOB NOT NULL,
            digest BLOB NOT NULL, summary TEXT NOT NULL, UNIQUE(workspace,candidate,run),
            FOREIGN KEY(workspace,candidate) REFERENCES improvement_candidates(workspace,id));
         CREATE INDEX IF NOT EXISTS improvement_evidence_page ON improvement_evidence(workspace,candidate,sequence);
         CREATE TABLE IF NOT EXISTS improvement_text_chunks (
            workspace BLOB NOT NULL, candidate BLOB NOT NULL, source BLOB NOT NULL,
            offset INTEGER NOT NULL, next INTEGER NOT NULL, source_digest BLOB NOT NULL,
            chunk_digest BLOB NOT NULL, body BLOB NOT NULL,
            PRIMARY KEY(workspace,candidate,source,offset),
            FOREIGN KEY(workspace,candidate) REFERENCES improvement_candidates(workspace,id));
         CREATE TABLE IF NOT EXISTS improvement_revisions (workspace BLOB PRIMARY KEY, revision INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS improvement_backfill (
            workspace BLOB NOT NULL, run BLOB NOT NULL, source_digest BLOB NOT NULL,
            candidate BLOB,
            PRIMARY KEY(workspace,run),
            FOREIGN KEY(workspace,candidate) REFERENCES improvement_candidates(workspace,id));",
    ).map_err(problem)?;
    if version == 2 {
        migrate_two(&transaction)?;
    } else {
        migrate_normalized_layout(&transaction)?;
    }
    reconcile_evaluation_runs(&transaction, version != super::CURRENT_SCHEMA)?;
    transaction
        .execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS improvement_candidates_evaluation_run
             ON improvement_candidates(evaluation_run) WHERE evaluation_run IS NOT NULL;",
        )
        .map_err(problem)?;
    verify_normalized(&transaction)?;
    transaction.pragma_update(None, "user_version", super::CURRENT_SCHEMA).map_err(problem)?;
    transaction.commit().map_err(problem)
}

fn migrate_normalized_layout(transaction: &Transaction<'_>) -> Result<(), Error> {
    if !has_column(transaction, "evidence_count")? {
        transaction
            .execute_batch(
                "ALTER TABLE improvement_candidates ADD COLUMN evidence_count INTEGER NOT NULL DEFAULT 0;
                 UPDATE improvement_candidates
                    SET evidence_count=(SELECT count(*) FROM improvement_evidence e
                        WHERE e.workspace=improvement_candidates.workspace
                          AND e.candidate=improvement_candidates.id);",
            )
            .map_err(problem)?;
    }
    if !has_column(transaction, "evaluation_run")? {
        transaction
            .execute_batch(
                "ALTER TABLE improvement_candidates ADD COLUMN evaluation_run BLOB;",
            )
            .map_err(problem)?;
    }
    Ok(())
}

fn has_column(transaction: &Transaction<'_>, name: &str) -> Result<bool, Error> {
    transaction
        .prepare("PRAGMA table_info(improvement_candidates)")
        .and_then(|mut statement| {
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                if row.get::<_, String>(1)? == name {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .map_err(problem)
}

fn migrate_two(transaction: &Transaction<'_>) -> Result<(), Error> {
    let legacy_candidates: u64 = transaction
        .query_row("SELECT count(*) FROM improvements", [], |row| row.get(0))
        .map_err(problem)?;
    let existing_candidates: u64 = transaction
        .query_row("SELECT count(*) FROM improvement_candidates", [], |row| row.get(0))
        .map_err(problem)?;
    let existing_evidence: u64 = transaction
        .query_row("SELECT count(*) FROM improvement_evidence", [], |row| row.get(0))
        .map_err(problem)?;
    if existing_candidates != 0 || existing_evidence != 0 {
        return Err(problem("schema-2 migration destination is not empty"));
    }

    // Only one legacy candidate is decoded at a time; original rows stay available until commit.
    let mut expected_evidence = 0_u64;
    {
        let mut statement = transaction.prepare("SELECT workspace,id,record FROM improvements ORDER BY rowid")
            .map_err(problem)?;
        let mut rows = statement.query([]).map_err(problem)?;
        while let Some(row) = rows.next().map_err(problem)? {
            let workspace: [u8; 16] = row.get(0).map_err(problem)?;
            let id: [u8; 32] = row.get(1).map_err(problem)?;
            let value: String = row.get(2).map_err(problem)?;
            let item: Candidate = serde_json::from_str(&value).map_err(problem)?;
            if item.workspace != workspace || item.id != id {
                return Err(problem("legacy improvement record scope mismatch"));
            }
            item.project()?;
            let evaluation = item.evaluation.as_ref().map(serde_json::to_string)
                .transpose().map_err(problem)?;
            let evaluation_run = item.evaluation.as_ref().map(|value| value.run);
            let evidence_count = u64::try_from(item.evidence.len()).map_err(problem)?;
            expected_evidence = expected_evidence
                .checked_add(evidence_count)
                .ok_or_else(|| problem("legacy improvement evidence count overflow"))?;
            transaction.execute(
                "INSERT INTO improvement_candidates(workspace,id,proposal,proposal_digest,evidence_count,evaluation,evaluation_run,dismissed) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![workspace, id, item.proposal, digest(&[b"peritus.improvement.proposal.v1", item.proposal.as_bytes()]), evidence_count, evaluation, evaluation_run, item.dismissed],
            ).map_err(problem)?;
            chunks::write(transaction, &workspace, &id, &[0; 16],
                digest(&[b"peritus.improvement.proposal.v1", item.proposal.as_bytes()]), &item.proposal)?;
            for evidence in &item.evidence {
                transaction.execute(
                    "INSERT INTO improvement_evidence(workspace,candidate,run,digest,summary) VALUES (?1,?2,?3,?4,?5)",
                    params![workspace, id, evidence.run, evidence.digest, evidence.summary],
                ).map_err(problem)?;
                chunks::write(transaction, &workspace, &id, &evidence.run, evidence.digest, &evidence.summary)?;
            }
            advance(transaction, &workspace)?;
        }
    }
    let migrated_candidates: u64 = transaction
        .query_row("SELECT count(*) FROM improvement_candidates", [], |row| row.get(0))
        .map_err(problem)?;
    let migrated_evidence: u64 = transaction
        .query_row("SELECT count(*) FROM improvement_evidence", [], |row| row.get(0))
        .map_err(problem)?;
    if migrated_candidates != legacy_candidates || migrated_evidence != expected_evidence {
        return Err(problem("schema-2 improvement migration replacement frontier is incomplete"));
    }
    transaction.execute_batch("DROP TABLE improvements;").map_err(problem)
}

/// Fills the new run index one row at a time. A duplicate run or malformed reservation aborts
/// the enclosing migration transaction without replacing the prior schema.
fn reconcile_evaluation_runs(
    transaction: &Transaction<'_>,
    may_update: bool,
) -> Result<(), Error> {
    let mut frontier = 0_u64;
    loop {
        let row: Option<(
            u64,
            [u8; 16],
            [u8; 32],
            Option<String>,
            Option<[u8; 16]>,
        )> = transaction
            .query_row(
                "SELECT sequence,workspace,id,evaluation,evaluation_run
                 FROM improvement_candidates WHERE sequence>?1 ORDER BY sequence LIMIT 1",
                [frontier],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(problem)?;
        let Some((sequence, workspace, id, raw, indexed)) = row else {
            return Ok(());
        };
        let expected = raw
            .map(|raw| {
                let evaluation: super::Evaluation = serde_json::from_str(&raw).map_err(problem)?;
                evaluation.project(workspace, id)?;
                Ok::<_, Error>(evaluation.run)
            })
            .transpose()?;
        if indexed != expected {
            if !may_update {
                return Err(problem("improvement evaluation run index mismatch"));
            }
            transaction
                .execute(
                    "UPDATE improvement_candidates SET evaluation_run=?2 WHERE sequence=?1",
                    params![sequence, expected],
                )
                .map_err(problem)?;
        }
        frontier = sequence;
    }
}

fn verify_normalized(transaction: &Transaction<'_>) -> Result<(), Error> {
    let invalid: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM improvement_candidates c
             WHERE c.evidence_count != (SELECT count(*) FROM improvement_evidence e
                WHERE e.workspace=c.workspace AND e.candidate=c.id)
                OR c.evidence_count < 1
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(problem)?;
    if invalid.is_some() {
        return Err(problem("normalized improvement evidence frontier is incomplete"));
    }
    Ok(())
}

pub(super) fn advance(transaction: &Transaction<'_>, workspace: &[u8; 16]) -> Result<(), Error> {
    transaction.execute(
        "INSERT INTO improvement_revisions(workspace,revision) VALUES (?1,1) ON CONFLICT(workspace) DO UPDATE SET revision=revision+1",
        [workspace],
    ).map_err(problem)?;
    Ok(())
}
