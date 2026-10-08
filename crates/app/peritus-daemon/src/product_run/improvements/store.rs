//! Transactional local inbox. Suggestions carry no execution or promotion authority.

use super::{Error, digest};
use peritus_app_protocol::{
    ConversationId, ImprovementCandidate, ImprovementEvaluation, ImprovementEvidence,
    ImprovementInbox, ImprovementText, ImprovementTextReference,
};
use peritus_types::{ActorId, RunId, Sha256Digest, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
};

std::thread_local! {
    static ACTIVE_CONTENTION: RefCell<Vec<ActiveContention>> = const {
        RefCell::new(Vec::new())
    };
}

mod schema;
mod paging;
mod chunks;
mod backfill;
mod sources;
mod reservations;

pub(super) use sources::{EvaluationSource, EvaluationSourcePage};

const CURRENT_SCHEMA: u32 = 5;
const PRE_RELEASE_SCHEMA: u32 = 1;
const SQLITE_SIDECARS: [&str; 3] = ["-wal", "-shm", "-journal"];

pub(in crate::product_run) struct Store(Connection);

struct ActiveContention {
    cancellation: peritus_journal::JournalCancellation,
    interrupted: bool,
}

/// Outcome of one startup operation under exact improvement-database contention cancellation.
pub(in crate::product_run) enum StartupContention<T> {
    /// The operation completed without an observed cancellation interrupt.
    Completed(T),
    /// The active startup token caused the busy callback to stop `SQLite`.
    Cancelled,
}

/// Runs startup work and reports cancellation only when its busy callback stopped `SQLite`.
pub(in crate::product_run) fn with_startup_cancellation<T>(
    cancellation: &peritus_journal::JournalCancellation,
    operation: impl FnOnce() -> Result<T, Error>,
) -> Result<StartupContention<T>, Error> {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE_CONTENTION.with(|active| {
                active.borrow_mut().pop();
            });
        }
    }

    ACTIVE_CONTENTION.with(|active| {
        active.borrow_mut().push(ActiveContention {
            cancellation: cancellation.clone(),
            interrupted: false,
        });
    });
    let restore = Restore;
    let result = operation();
    let interrupted = ACTIVE_CONTENTION.with(|active| {
        active.borrow().last().is_some_and(|owner| owner.interrupted)
    });
    drop(restore);
    match result {
        Err(_) if interrupted => Ok(StartupContention::Cancelled),
        result => result.map(StartupContention::Completed),
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evaluation {
    pub(super) actor: [u8; 16],
    pub(super) conversation: [u8; 16],
    pub(super) run: [u8; 16],
    pub(super) target: [u8; 16],
    pub(super) providers: [[u8; 16]; 3],
}

/// The exact durable evaluation reservation without eagerly materializing its retained bodies.
pub(super) struct Reservation {
    pub(super) id: [u8; 32],
    pub(super) workspace: WorkspaceId,
    pub(super) proposal: ImprovementTextReference,
    pub(super) evidence_count: u64,
    pub(super) evaluation: Evaluation,
}

impl Evaluation {
    fn project(&self, workspace: [u8; 16], id: [u8; 32]) -> Result<ImprovementEvaluation, Error> {
        let actor = ActorId::new(self.actor).map_err(|_| problem("invalid evaluation actor"))?;
        let conversation = ConversationId::new(self.conversation).map_err(|_| problem("invalid evaluation conversation"))?;
        let run = RunId::new(self.run).map_err(|_| problem("invalid evaluation run"))?;
        let target = WorkspaceId::new(self.target).map_err(|_| problem("invalid evaluation workspace"))?;
        let source = WorkspaceId::new(workspace).map_err(|_| problem("invalid source workspace"))?;
        if super::evaluation::derived_conversation(actor, source, id, run)? != conversation {
            return Err(problem("evaluation conversation identity mismatch"));
        }
        Ok(ImprovementEvaluation::new(conversation, run, target))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    run: [u8; 16],
    digest: [u8; 32],
    summary: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        let normalized = backfill::normalize(&self.proposal);
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
        let evaluation = self
            .evaluation
            .as_ref()
            .map(|e| e.project(self.workspace, self.id))
            .transpose()?;
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
            evaluation,
            self.dismissed,
        )
        .map_err(problem)
    }
}

impl Store {
    pub(in crate::product_run) fn open(path: &Path) -> Result<Self, Error> {
        let conn = connection(path)?;
        let version: u32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(problem)?;
        if version == PRE_RELEASE_SCHEMA {
            drop(conn);
            quarantine_pre_release(path)?;
            return initialize(connection(path)?);
        }
        if version != 0
            && version != 2
            && version != 3
            && version != 4
            && version != CURRENT_SCHEMA
        {
            return Err(problem(
                "unsupported improvement inbox schema; use the Peritus version that owns this state",
            ));
        }
        initialize(conn)
    }

    pub(super) fn collect(
        &mut self,
        workspace: WorkspaceId,
        run: RunId,
        proposal: &str,
        summary: &str,
    ) -> Result<[u8; 32], Error> {
        text(proposal)?;
        text(summary)?;
        let transaction = self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(problem)?;
        let id = backfill::collect_candidate(&transaction, workspace, run, proposal, summary)?;
        transaction.commit().map_err(problem)?;
        Ok(id)
    }

    pub(super) fn get(
        &self,
        workspace: WorkspaceId,
        id: [u8; 32],
    ) -> Result<Option<Candidate>, Error> {
        let value: Option<(String, [u8; 32], Option<String>, bool)> = self
            .0
            .query_row(
                "SELECT proposal,proposal_digest,evaluation,dismissed FROM improvement_candidates WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(problem)?;
        value
            .map(|(proposal, proposal_digest, evaluation, dismissed)| {
                if proposal_digest != digest(&[b"peritus.improvement.proposal.v1", proposal.as_bytes()]) {
                    return Err(problem("improvement proposal body digest mismatch"));
                }
                let mut statement = self.0.prepare(
                    "SELECT run,digest,summary FROM improvement_evidence WHERE workspace=?1 AND candidate=?2 ORDER BY sequence",
                ).map_err(problem)?;
                let evidence = statement.query_map(params![workspace.as_bytes(), id], |r| {
                    Ok(Evidence { run: r.get(0)?, digest: r.get(1)?, summary: r.get(2)? })
                }).map_err(problem)?.collect::<Result<Vec<_>, _>>().map_err(problem)?;
                let item = Candidate {
                    id, workspace: workspace.into_bytes(), proposal, evidence,
                    evaluation: evaluation.map(|value| serde_json::from_str(&value).map_err(problem)).transpose()?,
                    dismissed,
                };
                item.project()?;
                Ok(item)
            })
            .transpose()
    }

    pub(super) fn inbox(&self, workspace: WorkspaceId) -> Result<ImprovementInbox, Error> {
        let mut statement = self
            .0
            .prepare("SELECT id FROM improvement_candidates WHERE workspace=?1 ORDER BY dismissed, sequence DESC")
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
        let transaction = self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(problem)?;
        let exists: Option<bool> = transaction.query_row(
            "SELECT dismissed FROM improvement_candidates WHERE workspace=?1 AND id=?2",
            params![workspace.as_bytes(), id], |r| r.get(0),
        ).optional().map_err(problem)?;
        let dismissed = exists.ok_or(Error::NotFound)?;
        if !dismissed {
            transaction.execute(
                "UPDATE improvement_candidates SET dismissed=1 WHERE workspace=?1 AND id=?2",
                params![workspace.as_bytes(), id],
            ).map_err(problem)?;
            schema::advance(&transaction, workspace.as_bytes())?;
        }
        transaction.commit().map_err(problem)
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
            if prior.actor != evaluation.actor
                || prior.target != evaluation.target
                || prior.providers != evaluation.providers
            {
                return Err(Error::invalid_data(
                    "evaluate improvement",
                    "This suggestion already has an evaluation owned by another actor, workspace, or provider selection. Open its original durable conversation",
                ));
            }
        } else {
            item.evaluation = Some(evaluation);
            item.project()?;
            let value = serde_json::to_string(&item.evaluation).map_err(problem)?;
            let transaction = self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(problem)?;
            let changed = transaction.execute(
                "UPDATE improvement_candidates SET evaluation=?3 WHERE workspace=?1 AND id=?2 AND evaluation IS NULL AND dismissed=0",
                params![workspace.as_bytes(), id, value],
            ).map_err(problem)?;
            if changed != 1 { return Err(Error::InvalidState); }
            schema::advance(&transaction, workspace.as_bytes())?;
            transaction.commit().map_err(problem)?;
        }
        // Read after the durable freeze so an observation committed just before reservation is
        // included in the original evaluation, including with another process using this store.
        self.get(workspace, id)?.ok_or(Error::NotFound)
    }

    pub(super) fn evaluation_runs(&self, workspace: WorkspaceId) -> Result<std::collections::BTreeSet<[u8; 16]>, Error> {
        let mut statement = self.0.prepare(
            "SELECT evaluation FROM improvement_candidates WHERE workspace=?1 AND evaluation IS NOT NULL",
        ).map_err(problem)?;
        statement.query_map([workspace.as_bytes()], |r| r.get::<_, String>(0))
            .map_err(problem)?
            .map(|value| {
                let value: Evaluation = serde_json::from_str(&value.map_err(problem)?).map_err(problem)?;
                RunId::new(value.run).map_err(problem)?;
                Ok(value.run)
            }).collect()
    }
}

fn connection(path: &Path) -> Result<Connection, Error> {
    let connection = Connection::open(path).map_err(problem)?;
    connection.busy_handler(Some(wait_for_contention)).map_err(problem)?;
    Ok(connection)
}

fn wait_for_contention(_prior_attempts: i32) -> bool {
    let cancelled = ACTIVE_CONTENTION.with(|active| {
        let mut active = active.borrow_mut();
        active.last_mut().is_some_and(|owner| {
            if owner.cancellation.is_cancelled() {
                owner.interrupted = true;
                true
            } else {
                false
            }
        })
    });
    if cancelled {
        return false;
    }
    std::thread::sleep(std::time::Duration::from_millis(10));
    true
}

fn initialize(mut connection: Connection) -> Result<Store, Error> {
    schema::initialize(&mut connection)?;
    Ok(Store(connection))
}

fn quarantine_pre_release(path: &Path) -> Result<(), Error> {
    let parent =
        path.parent().ok_or_else(|| problem("improvement inbox has no parent directory"))?;
    let quarantine = parent.join("improvements-quarantine");
    fs::create_dir_all(&quarantine).map_err(problem)?;
    sync_directory(parent)?;
    let destination = available_quarantine_path(&quarantine, path);
    for suffix in SQLITE_SIDECARS {
        let source = sqlite_sidecar(path, suffix);
        if source.exists() {
            fs::rename(source, sqlite_sidecar(&destination, suffix)).map_err(problem)?;
        }
    }
    fs::rename(path, &destination).map_err(problem)?;
    sync_directory(&quarantine)?;
    sync_directory(parent)?;
    crate::diagnostic::report(&format!(
        "peritusd: isolated unsupported pre-release improvement inbox {} at {}",
        path.display(),
        destination.display()
    ));
    Ok(())
}

fn available_quarantine_path(directory: &Path, source: &Path) -> PathBuf {
    let name =
        source.file_name().and_then(|value| value.to_str()).unwrap_or("improvements.sqlite3");
    for suffix in 0_u32.. {
        let ending = if suffix == 0 { String::new() } else { format!(".{suffix}") };
        let candidate = directory.join(format!("{name}.schema-{PRE_RELEASE_SCHEMA}{ending}"));
        if !candidate.exists()
            && SQLITE_SIDECARS.iter().all(|suffix| !sqlite_sidecar(&candidate, suffix).exists())
        {
            return candidate;
        }
    }
    unreachable!("u32 quarantine suffixes are exhaustive")
}

fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), Error> {
    fs::File::open(path).and_then(|directory| directory.sync_all()).map_err(problem)
}

#[cfg(not(unix))]
const fn sync_directory(_path: &Path) -> Result<(), Error> {
    Ok(())
}

fn text(value: &str) -> Result<ImprovementText, Error> {
    // The enclosing codec and SQLite record retain physical bounds; inert domain text does not
    // impose a smaller cumulative work allowance.
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
