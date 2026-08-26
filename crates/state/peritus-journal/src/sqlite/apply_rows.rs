//! Static parameterized row insertion for one checked plan.

use crate::{AppendPlan, JournalError, JournalErrorKind};
use rusqlite::{Transaction, params};

pub(super) fn apply_rows(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(u64, u64), JournalError> {
    let positions = insert_events(transaction, plan)?;
    advance_heads(transaction, plan)?;
    install_state(transaction, plan, positions.1)?;
    install_registry(transaction, plan, positions.1)?;
    insert_artifact_references(transaction, plan, positions.1)?;
    insert_outbox(transaction, plan, positions.1)?;
    acknowledge_outbox(transaction, plan)?;
    Ok(positions)
}

fn insert_events(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(u64, u64), JournalError> {
    let mut first = None;
    let mut last = 0;
    for planned in &plan.events {
        let draft = &planned.draft;
        let causal_ids: Vec<u8> =
            draft.causal_parents().iter().flat_map(|id| id.as_bytes().iter().copied()).collect();
        transaction
            .execute(
                "INSERT INTO events(event_id, aggregate_kind, aggregate_id, sequence, previous_event_id, previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest, revision_digest, causal_ids, frame) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    draft.event_id().as_bytes().as_slice(),
                    draft.aggregate().kind().tag(),
                    draft.aggregate().id().as_bytes().as_slice(),
                    super::append::to_i64(draft.sequence().get(), "event sequence")?,
                    draft.previous_event_id().map(|id| id.as_bytes().to_vec()),
                    planned.previous_hash.as_bytes().as_slice(),
                    planned.event_hash.as_bytes().as_slice(),
                    plan.command_id.as_bytes().as_slice(),
                    i64::from(draft.frame().family()),
                    i64::from(draft.frame().schema_version()),
                    draft.frame().digest().as_bytes().as_slice(),
                    draft.revision_digest().as_bytes().as_slice(),
                    causal_ids,
                    draft.frame().bytes(),
                ],
            )
            .map_err(|error| JournalError::sqlite("insert event", error))?;
        let position = u64::try_from(transaction.last_insert_rowid()).map_err(|_| {
            JournalError::new(
                JournalErrorKind::SequenceOverflow,
                "insert event",
                "SQLite returned an invalid global position",
            )
        })?;
        first.get_or_insert(position);
        last = position;
    }
    Ok((first.expect("validated nonempty batch"), last))
}

fn advance_heads(transaction: &Transaction<'_>, plan: &AppendPlan) -> Result<(), JournalError> {
    for expected in &plan.heads {
        let final_event = plan
            .events
            .iter()
            .rev()
            .find(|event| event.draft.aggregate() == expected.key())
            .expect("validated head has an event");
        transaction
            .execute(
                "INSERT INTO aggregate_heads(aggregate_kind, aggregate_id, sequence, event_id, event_hash) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(aggregate_kind, aggregate_id) DO UPDATE SET sequence = excluded.sequence, event_id = excluded.event_id, event_hash = excluded.event_hash",
                params![
                    expected.key().kind().tag(),
                    expected.key().id().as_bytes().as_slice(),
                    super::append::to_i64(final_event.draft.sequence().get(), "aggregate sequence")?,
                    final_event.draft.event_id().as_bytes().as_slice(),
                    final_event.event_hash.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| JournalError::sqlite("advance aggregate head", error))?;
    }
    Ok(())
}

fn install_state(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
    position: u64,
) -> Result<(), JournalError> {
    for install in &plan.state_installs {
        transaction
            .execute(
                "INSERT INTO state_record_history(namespace, record_key, revision, value_digest, value, producing_position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    i64::from(install.namespace()),
                    install.key(),
                    super::append::to_i64(install.revision(), "state revision")?,
                    install.digest().as_bytes().as_slice(),
                    install.bytes(),
                    super::append::to_i64(position, "state producing position")?,
                ],
            )
            .map_err(|error| JournalError::sqlite("append state record history", error))?;
        transaction
            .execute(
                "INSERT INTO state_records(namespace, record_key, revision, value_digest, value, producing_position) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(namespace, record_key) DO UPDATE SET revision = excluded.revision, value_digest = excluded.value_digest, value = excluded.value, producing_position = excluded.producing_position",
                params![
                    i64::from(install.namespace()),
                    install.key(),
                    super::append::to_i64(install.revision(), "state revision")?,
                    install.digest().as_bytes().as_slice(),
                    install.bytes(),
                    super::append::to_i64(position, "state producing position")?,
                ],
            )
            .map_err(|error| JournalError::sqlite("install state record", error))?;
    }
    Ok(())
}

fn install_registry(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
    position: u64,
) -> Result<(), JournalError> {
    let Some(install) = &plan.registry_install else {
        return Ok(());
    };
    transaction
        .execute(
            "INSERT INTO credential_registry(singleton, revision, generation, snapshot_digest, snapshot, producing_position) VALUES (1, ?1, ?2, ?3, ?4, ?5) ON CONFLICT(singleton) DO UPDATE SET revision = excluded.revision, generation = excluded.generation, snapshot_digest = excluded.snapshot_digest, snapshot = excluded.snapshot, producing_position = excluded.producing_position",
            params![
                super::append::to_i64(install.revision(), "registry revision")?,
                super::append::to_i64(install.generation(), "credential generation")?,
                install.digest().as_bytes().as_slice(),
                install.snapshot_bytes(),
                super::append::to_i64(position, "registry producing position")?,
            ],
        )
        .map_err(|error| JournalError::sqlite("install credential registry", error))?;
    Ok(())
}

fn insert_artifact_references(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
    _position: u64,
) -> Result<(), JournalError> {
    for dependency in &plan.artifact_dependencies {
        peritus_artifact_store::sqlite_interop::insert_reference(
            transaction,
            peritus_artifact_store::ReferenceOwner::journal(plan.batch_hash),
            peritus_artifact_store::ArtifactDigest::from_sha256(dependency.digest()),
        )
        .map_err(|error| JournalError::sqlite("insert artifact reference", error))?;
    }
    Ok(())
}

fn insert_outbox(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
    position: u64,
) -> Result<(), JournalError> {
    for entry in &plan.outbox {
        transaction
            .execute(
                "INSERT INTO outbox(outbox_id, producing_position, destination, payload, max_attempts, state) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![
                    entry.id().as_bytes().as_slice(),
                    super::append::to_i64(position, "outbox producing position")?,
                    entry.destination(),
                    entry.payload(),
                    i64::from(entry.max_attempts()),
                ],
            )
            .map_err(|error| JournalError::sqlite("insert outbox message", error))?;
    }
    Ok(())
}

fn acknowledge_outbox(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
) -> Result<(), JournalError> {
    for acknowledgement in &plan.outbox_acknowledgements {
        let affected = transaction
            .execute(
                "UPDATE outbox SET state = 3, lease_until = NULL WHERE outbox_id = ?1 AND state = 2 AND fence = ?2",
                params![
                    acknowledgement.id().as_bytes().as_slice(),
                    super::append::to_i64(acknowledgement.fence(), "outbox fence")?,
                ],
            )
            .map_err(|error| JournalError::sqlite("acknowledge outbox during append", error))?;
        if affected != 1 {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "acknowledge outbox during append",
                "validated outbox acknowledgement changed inside one transaction",
            ));
        }
    }
    Ok(())
}
