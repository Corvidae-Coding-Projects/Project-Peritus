//! Transaction-local durable precondition observations.

use crate::{AppendPlan, ExpectedAuthorityEpoch, HeadExpectation, JournalError, JournalErrorKind};
use rusqlite::{OptionalExtension, Transaction, params};

pub(super) fn verify_all(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(), JournalError> {
    verify_heads(transaction, plan)?;
    verify_artifacts(transaction, plan)?;
    verify_authority(transaction, plan)?;
    verify_registry(transaction, plan)?;
    verify_state_installs(transaction, plan)?;
    verify_outbox_acknowledgements(transaction, plan)
}

fn verify_heads(transaction: &Transaction<'_>, plan: &AppendPlan) -> Result<(), JournalError> {
    for expected in &plan.heads {
        let key = expected.key();
        let observed: Option<(i64, Vec<u8>, Vec<u8>)> = transaction
            .query_row(
                "SELECT sequence, event_id, event_hash FROM aggregate_heads WHERE aggregate_kind = ?1 AND aggregate_id = ?2",
                params![key.kind().tag(), key.id().as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("observe aggregate head", error))?;
        let matches = match (*expected, observed) {
            (HeadExpectation::Absent(_), None) => true,
            (HeadExpectation::Present(head), Some((sequence, event_id, event_hash))) => {
                u64::try_from(sequence).ok() == Some(head.sequence().get())
                    && event_id.as_slice() == head.event_id().as_bytes()
                    && event_hash.as_slice() == head.event_hash().as_bytes()
            }
            _ => false,
        };
        if !matches {
            return Err(JournalError::new(
                JournalErrorKind::StaleHead,
                "compare aggregate head",
                "stored aggregate head differs from append plan",
            ));
        }
    }
    Ok(())
}

fn verify_artifacts(transaction: &Transaction<'_>, plan: &AppendPlan) -> Result<(), JournalError> {
    for dependency in &plan.artifact_dependencies {
        let exists = peritus_artifact_store::sqlite_interop::is_referenceable(
            transaction,
            peritus_artifact_store::ArtifactDigest::from_sha256(dependency.digest()),
        )
        .map_err(|error| JournalError::sqlite("observe artifact dependency", error))?;
        if !exists {
            return Err(JournalError::new(
                JournalErrorKind::MissingArtifact,
                "verify artifact dependency",
                "required artifact is absent, partial, or quarantined",
            ));
        }
    }
    Ok(())
}

fn verify_authority(transaction: &Transaction<'_>, plan: &AppendPlan) -> Result<(), JournalError> {
    let Some(expected) = plan.expected_authority_epoch else {
        return Ok(());
    };
    let observed: Option<i64> = transaction
        .query_row("SELECT current_epoch FROM authority_clock WHERE singleton = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| JournalError::sqlite("observe authority epoch", error))?;
    let matches = match (expected, observed) {
        (ExpectedAuthorityEpoch::Absent, None) => true,
        (ExpectedAuthorityEpoch::Current(epoch), Some(value)) => {
            u64::try_from(value).ok() == Some(epoch.get())
        }
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(JournalError::new(
            JournalErrorKind::StaleAuthorityEpoch,
            "compare authority epoch",
            "stored authority epoch differs from append plan",
        ))
    }
}

fn verify_registry(transaction: &Transaction<'_>, plan: &AppendPlan) -> Result<(), JournalError> {
    if plan.expected_registry.is_none() && plan.registry_install.is_none() {
        return Ok(());
    }
    let observed: Option<(i64, i64, Vec<u8>)> = transaction
        .query_row(
            "SELECT revision, generation, snapshot_digest FROM credential_registry WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| JournalError::sqlite("observe credential registry", error))?;
    if let Some(expected) = plan.expected_registry {
        let current = observed.as_ref().is_some_and(|(revision, generation, digest)| {
            u64::try_from(*revision).ok() == Some(expected.revision)
                && u64::try_from(*generation).ok() == Some(expected.generation)
                && digest.as_slice() == expected.digest.as_bytes()
        });
        if !current {
            return Err(stale_registry());
        }
    }
    if let Some(install) = &plan.registry_install {
        let matches = match (install.expected_revision(), observed) {
            (None, None) => crate::verified::registry_advance(
                None,
                install.revision(),
                None,
                install.generation(),
            ),
            (Some(expected), Some((revision, generation, _))) => {
                let revision = u64::try_from(revision).ok();
                let generation = u64::try_from(generation).ok();
                revision == Some(expected)
                    && crate::verified::registry_advance(
                        revision,
                        install.revision(),
                        generation,
                        install.generation(),
                    )
            }
            _ => false,
        };
        if !matches {
            return Err(stale_registry());
        }
    }
    Ok(())
}

const fn stale_registry() -> JournalError {
    JournalError::new(
        JournalErrorKind::StaleRegistry,
        "compare credential registry",
        "stored registry revision, generation, or digest differs from append plan",
    )
}

fn verify_state_installs(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(), JournalError> {
    for install in &plan.state_installs {
        let observed: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM state_records WHERE namespace = ?1 AND record_key = ?2",
                params![i64::from(install.namespace()), install.key()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("observe state record", error))?;
        let matches = match (install.expected_revision(), observed) {
            (None, None) => true,
            (Some(expected), Some(value)) => u64::try_from(value).ok() == Some(expected),
            _ => false,
        };
        if !matches {
            return Err(JournalError::new(
                JournalErrorKind::StaleHead,
                "compare state record",
                "stored state revision differs from append plan",
            ));
        }
    }
    Ok(())
}

fn verify_outbox_acknowledgements(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(), JournalError> {
    for acknowledgement in &plan.outbox_acknowledgements {
        let observed: Option<(i64, Option<i64>)> = transaction
            .query_row(
                "SELECT state, fence FROM outbox WHERE outbox_id = ?1",
                params![acknowledgement.id().as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("observe outbox acknowledgement", error))?;
        let Some((state, fence)) = observed else {
            return Err(JournalError::new(
                JournalErrorKind::NotFound,
                "compare outbox acknowledgement",
                "outbox identity does not exist",
            ));
        };
        if state != 2
            || fence.and_then(|value| u64::try_from(value).ok()) != Some(acknowledgement.fence())
        {
            return Err(JournalError::new(
                JournalErrorKind::StaleHead,
                "compare outbox acknowledgement",
                "outbox row is not claimed under the supplied fence",
            ));
        }
    }
    Ok(())
}
