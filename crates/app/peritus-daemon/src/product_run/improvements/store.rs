//! Transactional local inbox. Suggestions carry no execution or promotion authority.

use super::{Error, digest};
use peritus_app_protocol::{
    ImprovementCandidate, ImprovementEvidence, ImprovementInbox, ImprovementText,
    MAX_IMPROVEMENT_EVIDENCE, MAX_IMPROVEMENTS,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(in crate::product_run) struct Store(Connection);

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Evaluation {
    pub(super) run: [u8; 16],
    pub(super) target: [u8; 16],
    pub(super) providers: [[u8; 16]; 3],
}

#[derive(Clone, Serialize, Deserialize)]
struct Evidence {
    run: [u8; 16],
    digest: [u8; 32],
    summary: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Candidate {
    pub(super) id: [u8; 32],
    pub(super) workspace: [u8; 16],
    pub(super) proposal: String,
    evidence: Vec<Evidence>,
    pub(super) evaluation: Option<Evaluation>,
    dismissed: bool,
}

impl Candidate {
    fn project(&self) -> Result<ImprovementCandidate, Error> {
        let normalized =
            self.proposal.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if self.id != digest(&[b"peritus.improvement.v1", &self.workspace, normalized.as_bytes()])
            || self.evidence.iter().any(|e| {
                e.digest
                    != digest(&[
                        b"peritus.improvement.observation.v1",
                        &e.run,
                        e.summary.as_bytes(),
                    ])
            })
        {
            return Err(problem("improvement evidence or proposal digest mismatch"));
        }
        ImprovementCandidate::new(
            Sha256Digest::new(self.id),
            text(&self.proposal)?,
            self.evidence
                .iter()
                .map(|e| {
                    Ok(ImprovementEvidence::new(
                        RunId::new(e.run).map_err(|_| problem("invalid evidence run"))?,
                        Sha256Digest::new(e.digest),
                        text(&e.summary)?,
                    ))
                })
                .collect::<Result<_, Error>>()?,
            self.evaluation
                .as_ref()
                .map(|e| RunId::new(e.run).map_err(|_| problem("invalid evidence run")))
                .transpose()?,
            self.dismissed,
        )
        .map_err(problem)
    }
    pub(super) fn evidence_prompt(&self) -> String {
        self.evidence
            .iter()
            .map(|e| {
                format!("Run {} (observation {}):\n{}", hex(&e.run), hex(&e.digest), e.summary)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Store {
    pub(in crate::product_run) fn open(path: &Path) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(problem)?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(problem)?;
        let version: u32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(problem)?;
        if version > 1 {
            return Err(problem("unsupported improvement inbox schema"));
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS improvements (workspace BLOB NOT NULL, id BLOB NOT NULL, record TEXT NOT NULL, PRIMARY KEY(workspace,id)); PRAGMA user_version=1;").map_err(problem)?;
        Ok(Self(conn))
    }

    pub(super) fn collect(
        &mut self,
        workspace: WorkspaceId,
        run: RunId,
        proposal: &str,
        summary: &str,
    ) -> Result<(), Error> {
        text(proposal)?;
        text(summary)?;
        let normalized = proposal.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        let id = digest(&[b"peritus.improvement.v1", workspace.as_bytes(), normalized.as_bytes()]);
        let mut item = if let Some(item) = self.get(workspace, id)? {
            item
        } else {
            let count: u32 = self
                    .0
                    .query_row(
                        "SELECT count(*) FROM improvements WHERE workspace=?1 AND json_extract(record, '$.dismissed')=0",
                        [workspace.as_bytes()],
                        |r| r.get(0),
                    )
                    .map_err(problem)?;
            if count as usize >= MAX_IMPROVEMENTS {
                return Err(Error::Control(
                    peritus_product_runner::control::ControlError::Capacity,
                ));
            }
            Candidate {
                id,
                workspace: workspace.into_bytes(),
                proposal: proposal.into(),
                evidence: Vec::new(),
                evaluation: None,
                dismissed: false,
            }
        };
        // Once evaluated, its evidence is frozen. Dismissal also survives later observations.
        if item.evaluation.is_some()
            || item.evidence.iter().any(|e| e.run == run.into_bytes())
            || item.evidence.len() == MAX_IMPROVEMENT_EVIDENCE
        {
            return Ok(());
        }
        item.evidence.push(Evidence {
            run: run.into_bytes(),
            digest: digest(&[
                b"peritus.improvement.observation.v1",
                run.as_bytes(),
                summary.as_bytes(),
            ]),
            summary: summary.into(),
        });
        self.save(&item)
    }

    pub(super) fn get(
        &self,
        workspace: WorkspaceId,
        id: [u8; 32],
    ) -> Result<Option<Candidate>, Error> {
        let value: Option<String> = self
            .0
            .query_row(
                "SELECT record FROM improvements WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
                |r| r.get(0),
            )
            .optional()
            .map_err(problem)?;
        value
            .map(|value| {
                if value.len() > 80_000 {
                    return Err(problem("oversized improvement record"));
                }
                let item: Candidate = serde_json::from_str(&value).map_err(problem)?;
                if item.id != id || item.workspace != workspace.into_bytes() {
                    return Err(problem("improvement record scope mismatch"));
                }
                item.project()?;
                Ok(item)
            })
            .transpose()
    }

    pub(super) fn inbox(&self, workspace: WorkspaceId) -> Result<ImprovementInbox, Error> {
        let mut statement = self
            .0
            .prepare("SELECT id FROM improvements WHERE workspace=?1 ORDER BY json_extract(record, '$.dismissed'), rowid DESC LIMIT 32")
            .map_err(problem)?;
        let ids = statement
            .query_map([workspace.as_bytes()], |row| row.get::<_, [u8; 32]>(0))
            .map_err(problem)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(problem)?;
        let candidates = ids
            .into_iter()
            .map(|id| self.get(workspace, id)?.ok_or(Error::NotFound)?.project())
            .collect::<Result<_, _>>()?;
        ImprovementInbox::new(workspace, candidates).map_err(problem)
    }

    pub(super) fn dismiss(&mut self, workspace: WorkspaceId, id: [u8; 32]) -> Result<(), Error> {
        let mut item = self.get(workspace, id)?.ok_or(Error::NotFound)?;
        item.dismissed = true;
        self.save(&item)
    }

    pub(super) fn reserve(
        &mut self,
        workspace: WorkspaceId,
        id: [u8; 32],
        evaluation: Evaluation,
    ) -> Result<Candidate, Error> {
        let mut item = self.get(workspace, id)?.ok_or(Error::NotFound)?;
        if item.dismissed {
            return Err(Error::invalid_data(
                "evaluate improvement",
                "This suggestion was dismissed",
            ));
        }
        if let Some(prior) = &item.evaluation {
            if prior.target != evaluation.target || prior.providers != evaluation.providers {
                return Err(Error::invalid_data(
                    "evaluate improvement",
                    "This suggestion already has an evaluation with different workspace or providers. Open its original run",
                ));
            }
        } else {
            item.evaluation = Some(evaluation);
            self.save(&item)?;
        }
        Ok(item)
    }

    fn save(&mut self, item: &Candidate) -> Result<(), Error> {
        item.project()?;
        let value = serde_json::to_string(item).map_err(problem)?;
        let transaction = self.0.transaction().map_err(problem)?;
        transaction.execute("INSERT INTO improvements(workspace,id,record) VALUES (?1,?2,?3) ON CONFLICT(workspace,id) DO UPDATE SET record=excluded.record", params![item.workspace, item.id, value]).map_err(problem)?;
        transaction.commit().map_err(problem)
    }
}

fn text(value: &str) -> Result<ImprovementText, Error> {
    ImprovementText::new(value.to_owned()).map_err(problem)
}
fn problem(error: impl std::fmt::Display) -> Error {
    Error::internal("read or write improvement inbox", error.to_string())
}
pub(super) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [char::from(HEX[usize::from(b >> 4)]), char::from(HEX[usize::from(b & 15)])])
        .collect()
}

#[cfg(test)]
mod tests;
