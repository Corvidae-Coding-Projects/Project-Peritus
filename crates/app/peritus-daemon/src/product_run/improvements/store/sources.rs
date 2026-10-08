//! Stable evaluation-source traversal over the evidence set frozen by reservation.

use super::{Error, Evaluation, Reservation, Store, problem};
use peritus_app_protocol::{
    IMPROVEMENT_PAGE_ITEMS, ImprovementTextQuery, ImprovementTextReference,
};
use peritus_types::{RunId, Sha256Digest};
use rusqlite::{OptionalExtension, params};

/// One exact retained proposal or observation body in deterministic evaluation order.
#[derive(Clone, Copy)]
pub(super) struct EvaluationSource {
    ordinal: u64,
    run: Option<RunId>,
    body: ImprovementTextReference,
}

impl EvaluationSource {
    pub(super) const fn ordinal(self) -> u64 {
        self.ordinal
    }

    pub(super) const fn run(self) -> Option<RunId> {
        self.run
    }

    pub(super) const fn body(self) -> ImprovementTextReference {
        self.body
    }

    pub(super) fn query(
        self,
        reservation: &Reservation,
        offset: u64,
    ) -> Result<ImprovementTextQuery, Error> {
        ImprovementTextQuery::new(
            reservation.workspace,
            Sha256Digest::new(reservation.id),
            self.run,
            self.body,
            offset,
        )
        .map_err(problem)
    }
}

/// One bounded metadata page and its indexed evidence continuation.
pub(super) struct EvaluationSourcePage {
    sources: Vec<EvaluationSource>,
    next: Option<u64>,
}

impl EvaluationSourcePage {
    pub(super) fn sources(&self) -> &[EvaluationSource] {
        &self.sources
    }

    pub(super) const fn next(&self) -> Option<u64> {
        self.next
    }
}

impl Store {
    /// Reads a bounded source page after its last source ordinal without using the mutable
    /// workspace display revision.
    /// Reservation freezes the indexed evidence set, while dismissal remains independent.
    pub(super) fn evaluation_sources(
        &mut self,
        reservation: &Reservation,
        after_ordinal: Option<u64>,
    ) -> Result<EvaluationSourcePage, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        validate_reservation(&transaction, reservation)?;
        let after = after_ordinal
            .map(|ordinal| ordinal.checked_sub(1).ok_or(Error::InvalidState))
            .transpose()?
            .unwrap_or(0);
        if after > i64::MAX as u64 {
            return Err(Error::InvalidState);
        }
        let mut sources = Vec::with_capacity(IMPROVEMENT_PAGE_ITEMS);
        if after_ordinal.is_none() {
            sources.push(EvaluationSource {
                ordinal: 1,
                run: None,
                body: reservation.proposal,
            });
        }
        let remaining = IMPROVEMENT_PAGE_ITEMS - sources.len();
        let mut statement = transaction
            .prepare(
                "SELECT sequence,run,digest,length(CAST(summary AS BLOB))
                 FROM improvement_evidence
                 WHERE workspace=?1 AND candidate=?2 AND sequence>?3
                 ORDER BY sequence LIMIT ?4",
            )
            .map_err(problem)?;
        let mut rows = statement
            .query(params![
                reservation.workspace.as_bytes(),
                reservation.id,
                after,
                u64::try_from(remaining + 1).map_err(problem)?,
            ])
            .map_err(problem)?;
        let mut last = None;
        let mut more = false;
        while let Some(row) = rows.next().map_err(problem)? {
            if sources.len() == IMPROVEMENT_PAGE_ITEMS {
                more = true;
                break;
            }
            let sequence: u64 = row.get(0).map_err(problem)?;
            let source = source_from_row(
                sequence,
                row.get(1).map_err(problem)?,
                row.get(2).map_err(problem)?,
                row.get(3).map_err(problem)?,
            )?;
            last = Some(source.ordinal());
            sources.push(source);
        }
        Ok(EvaluationSourcePage {
            sources,
            next: if more { Some(last.ok_or(Error::InvalidState)?) } else { None },
        })
    }

    /// Resolves one exact source ordinal under the frozen reservation.
    pub(super) fn evaluation_source(
        &mut self,
        reservation: &Reservation,
        ordinal: u64,
    ) -> Result<EvaluationSource, Error> {
        let transaction = self.0.transaction().map_err(problem)?;
        validate_reservation(&transaction, reservation)?;
        if ordinal == 1 {
            return Ok(EvaluationSource { ordinal, run: None, body: reservation.proposal });
        }
        let sequence = ordinal.checked_sub(1).ok_or(Error::InvalidState)?;
        if sequence > i64::MAX as u64 {
            return Err(Error::InvalidState);
        }
        let value: Option<([u8; 16], [u8; 32], u64)> = transaction
            .query_row(
                "SELECT run,digest,length(CAST(summary AS BLOB))
                 FROM improvement_evidence
                 WHERE workspace=?1 AND candidate=?2 AND sequence=?3",
                params![reservation.workspace.as_bytes(), reservation.id, sequence],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(problem)?;
        let (run, digest, bytes) = value.ok_or(Error::NotFound)?;
        source_from_row(sequence, run, digest, bytes)
    }
}

fn validate_reservation(
    transaction: &rusqlite::Transaction<'_>,
    reservation: &Reservation,
) -> Result<(), Error> {
        let retained: Option<(Option<String>, Option<[u8; 16]>, u64, [u8; 32], u64)> =
            transaction
            .query_row(
                "SELECT evaluation,evaluation_run,evidence_count,proposal_digest,length(CAST(proposal AS BLOB))
                 FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![reservation.workspace.as_bytes(), reservation.id],
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
        let (evaluation, evaluation_run, evidence_count, proposal_digest, proposal_bytes) =
            retained.ok_or(Error::NotFound)?;
        let evaluation = evaluation
            .map(|value| serde_json::from_str::<Evaluation>(&value).map_err(problem))
            .transpose()?
            .ok_or(Error::InvalidState)?;
        if evaluation != reservation.evaluation
            || evaluation_run != Some(reservation.evaluation.run)
            || evidence_count != reservation.evidence_count
            || proposal_digest != reservation.proposal.digest().into_bytes()
            || proposal_bytes != reservation.proposal.bytes()
        {
            return Err(problem("frozen improvement evaluation sources changed"));
        }
        Ok(())
}

fn source_from_row(
    sequence: u64,
    run: [u8; 16],
    digest: [u8; 32],
    bytes: u64,
) -> Result<EvaluationSource, Error> {
    Ok(EvaluationSource {
        ordinal: sequence.checked_add(1).ok_or(Error::InvalidState)?,
        run: Some(RunId::new(run).map_err(problem)?),
        body: ImprovementTextReference::new(Sha256Digest::new(digest), bytes).map_err(problem)?,
    })
}
