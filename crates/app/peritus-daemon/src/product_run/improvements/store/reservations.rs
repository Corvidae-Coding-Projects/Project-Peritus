//! Indexed, race-safe ownership of one durable evaluation run per candidate.

use super::{Evaluation, Error, Reservation, Store, digest, problem, schema};
use peritus_app_protocol::ImprovementTextReference;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

impl Store {
    pub(super) fn reserve(
        &mut self,
        workspace: WorkspaceId,
        id: [u8; 32],
        evaluation: Evaluation,
    ) -> Result<Reservation, Error> {
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(problem)?;
        let retained: Option<(
            String,
            [u8; 32],
            u64,
            Option<String>,
            Option<[u8; 16]>,
            bool,
        )> = transaction
            .query_row(
                "SELECT proposal,proposal_digest,evidence_count,evaluation,evaluation_run,dismissed
                 FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()
            .map_err(problem)?;
        let (proposal, proposal_digest, evidence_count, prior, evaluation_run, dismissed) =
            retained.ok_or(Error::NotFound)?;
        if proposal_digest != digest(&[b"peritus.improvement.proposal.v1", proposal.as_bytes()]) {
            return Err(problem("improvement proposal body digest mismatch"));
        }
        if dismissed {
            return Err(Error::invalid_data(
                "evaluate improvement",
                "This suggestion was dismissed",
            ));
        }
        let prior = prior
            .map(|value| serde_json::from_str::<Evaluation>(&value).map_err(problem))
            .transpose()?;
        let accepted = if let Some(prior) = prior {
            prior.project(workspace.into_bytes(), id)?;
            if evaluation_run != Some(prior.run) {
                return Err(problem("improvement evaluation run index mismatch"));
            }
            if prior.actor != evaluation.actor
                || prior.target != evaluation.target
                || prior.providers != evaluation.providers
            {
                return Err(Error::invalid_data(
                    "evaluate improvement",
                    "This suggestion already has an evaluation owned by another actor, workspace, or provider selection. Open its original durable conversation",
                ));
            }
            prior
        } else {
            if evaluation_run.is_some() {
                return Err(problem("unowned improvement evaluation run index"));
            }
            evaluation.project(workspace.into_bytes(), id)?;
            let conflict: Option<[u8; 32]> = transaction
                .query_row(
                    "SELECT id FROM improvement_candidates WHERE evaluation_run=?1 LIMIT 1",
                    [evaluation.run],
                    |row| row.get(0),
                )
                .optional()
                .map_err(problem)?;
            if conflict.is_some() {
                return Err(Error::invalid_data(
                    "evaluate improvement",
                    "Choose a new evaluation run identity; that run already owns another suggestion",
                ));
            }
            let value = serde_json::to_string(&evaluation).map_err(problem)?;
            let changed = transaction
                .execute(
                    "UPDATE improvement_candidates SET evaluation=?3,evaluation_run=?4
                     WHERE workspace=?1 AND id=?2 AND evaluation IS NULL
                       AND evaluation_run IS NULL AND dismissed=0",
                    params![workspace.as_bytes(), id, value, evaluation.run],
                )
                .map_err(problem)?;
            if changed != 1 {
                return Err(Error::InvalidState);
            }
            schema::advance(&transaction, workspace.as_bytes())?;
            evaluation
        };
        transaction.commit().map_err(problem)?;
        Ok(Reservation {
            id,
            workspace,
            proposal: ImprovementTextReference::new(
                Sha256Digest::new(proposal_digest),
                u64::try_from(proposal.len()).map_err(problem)?,
            )
            .map_err(problem)?,
            evidence_count,
            evaluation: accepted,
        })
    }

    /// Returns the exact reservation for a candidate without loading any retained body.
    pub(super) fn reserved_evaluation(
        &self,
        workspace: WorkspaceId,
        id: [u8; 32],
    ) -> Result<Option<Reservation>, Error> {
        let row: Option<([u8; 32], u64, u64, Option<String>, Option<[u8; 16]>)> = self
            .0
            .query_row(
                "SELECT proposal_digest,length(CAST(proposal AS BLOB)),evidence_count,evaluation,evaluation_run
                 FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(problem)?;
        row.map(|(proposal, bytes, evidence_count, evaluation, evaluation_run)| {
            reservation(
                workspace,
                id,
                proposal,
                bytes,
                evidence_count,
                evaluation,
                evaluation_run,
            )
        })
        .transpose()
        .map(Option::flatten)
    }

    /// Resolves a run to its reservation through the durable unique index.
    pub(super) fn reservation_for_run(
        &self,
        run: RunId,
    ) -> Result<Option<Reservation>, Error> {
        let row: Option<(
            [u8; 16],
            [u8; 32],
            [u8; 32],
            u64,
            u64,
            Option<String>,
            Option<[u8; 16]>,
        )> = self
            .0
            .query_row(
                "SELECT workspace,id,proposal_digest,length(CAST(proposal AS BLOB)),evidence_count,evaluation,evaluation_run
                 FROM improvement_candidates WHERE evaluation_run=?1",
                [run.as_bytes()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(problem)?;
        row.map(
            |(workspace, id, proposal, bytes, evidence_count, evaluation, evaluation_run)| {
                reservation(
                    WorkspaceId::new(workspace).map_err(problem)?,
                    id,
                    proposal,
                    bytes,
                    evidence_count,
                    evaluation,
                    evaluation_run,
                )
            },
        )
        .transpose()
        .map(Option::flatten)
    }
}

fn reservation(
    workspace: WorkspaceId,
    id: [u8; 32],
    proposal: [u8; 32],
    proposal_bytes: u64,
    evidence_count: u64,
    evaluation: Option<String>,
    evaluation_run: Option<[u8; 16]>,
) -> Result<Option<Reservation>, Error> {
    let Some(evaluation) = evaluation else {
        if evaluation_run.is_some() {
            return Err(problem("unowned improvement evaluation run index"));
        }
        return Ok(None);
    };
    let evaluation: Evaluation = serde_json::from_str(&evaluation).map_err(problem)?;
    evaluation.project(workspace.into_bytes(), id)?;
    if evaluation_run != Some(evaluation.run) {
        return Err(problem("improvement evaluation run index mismatch"));
    }
    Ok(Some(Reservation {
        id,
        workspace,
        proposal: ImprovementTextReference::new(Sha256Digest::new(proposal), proposal_bytes)
            .map_err(problem)?,
        evidence_count,
        evaluation,
    }))
}
