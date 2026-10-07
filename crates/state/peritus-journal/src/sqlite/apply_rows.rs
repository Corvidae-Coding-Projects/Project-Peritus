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
        let frame_byte_length = super::content::install(
            transaction,
            draft.frame().bytes(),
            draft.frame().digest(),
            crate::MAX_EVENT_FRAME_BYTES,
        )?;
        let causal_ids: Vec<u8> =
            draft.causal_parents().iter().flat_map(|id| id.as_bytes().iter().copied()).collect();
        transaction
            .execute(
                "INSERT INTO events(event_id, aggregate_kind, aggregate_id, sequence, previous_event_id, previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest, revision_digest, causal_ids, frame, frame_byte_length) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                    super::content::PAGED_INLINE_VALUE,
                    frame_byte_length,
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
        let root = super::history::install(transaction, install.bytes())?;
        transaction
            .execute(
                "INSERT INTO state_record_history(namespace, record_key, revision, value_digest, producing_position, root_digest) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    i64::from(install.namespace()),
                    install.key(),
                    super::append::to_i64(install.revision(), "state revision")?,
                    install.digest().as_bytes().as_slice(),
                    super::append::to_i64(position, "state producing position")?,
                    root.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| JournalError::sqlite("append state record history", error))?;
        transaction
            .execute(
                "INSERT INTO state_records(namespace, record_key, revision, value_digest, value, root_digest, value_bytes, producing_position) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(namespace, record_key) DO UPDATE SET revision = excluded.revision, value_digest = excluded.value_digest, value = excluded.value, root_digest = excluded.root_digest, value_bytes = excluded.value_bytes, producing_position = excluded.producing_position",
                params![
                    i64::from(install.namespace()),
                    install.key(),
                    super::append::to_i64(install.revision(), "state revision")?,
                    install.digest().as_bytes().as_slice(),
                    Vec::<u8>::new(),
                    root.as_bytes().as_slice(),
                    super::append::to_i64(
                        install.bytes().len() as u64,
                        "state value length",
                    )?,
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
    let snapshot_digest = peritus_codec::sha256(install.snapshot_bytes());
    let snapshot_byte_length = super::content::install(
        transaction,
        install.snapshot_bytes(),
        snapshot_digest,
        crate::MAX_EVENT_FRAME_BYTES,
    )?;
    transaction
        .execute(
            "INSERT INTO credential_registry(singleton, revision, generation, snapshot_digest, snapshot, snapshot_content_digest, snapshot_byte_length, producing_position) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(singleton) DO UPDATE SET revision = excluded.revision, generation = excluded.generation, snapshot_digest = excluded.snapshot_digest, snapshot = excluded.snapshot, snapshot_content_digest = excluded.snapshot_content_digest, snapshot_byte_length = excluded.snapshot_byte_length, producing_position = excluded.producing_position",
            params![
                super::append::to_i64(install.revision(), "registry revision")?,
                super::append::to_i64(install.generation(), "credential generation")?,
                install.digest().as_bytes().as_slice(),
                super::content::PAGED_INLINE_VALUE,
                snapshot_digest.as_bytes().as_slice(),
                snapshot_byte_length,
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
        let referenceable = peritus_artifact_store::sqlite_interop::insert_reference(
            transaction,
            peritus_artifact_store::ReferenceOwner::journal(plan.batch_hash),
            peritus_artifact_store::ArtifactDigest::from_sha256(dependency.digest()),
        )
        .map_err(|error| JournalError::sqlite("insert artifact reference", error))?;
        if !referenceable {
            return Err(JournalError::new(
                JournalErrorKind::MissingArtifact,
                "insert artifact dependency closure",
                "required artifact dependency closure is absent, inactive, or inconsistent",
            ));
        }
    }
    Ok(())
}

fn insert_outbox(
    transaction: &Transaction<'_>,
    plan: &AppendPlan,
    position: u64,
) -> Result<(), JournalError> {
    for entry in &plan.outbox {
        let payload_digest = peritus_codec::sha256(entry.payload());
        let payload_byte_length = super::content::install(
            transaction,
            entry.payload(),
            payload_digest,
            crate::outbox::MAX_OUTBOX_PAYLOAD_BYTES,
        )?;
        transaction
            .execute(
                "INSERT INTO outbox(outbox_id, producing_position, destination, payload, payload_digest, payload_byte_length, max_attempts, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
                params![
                    entry.id().as_bytes().as_slice(),
                    super::append::to_i64(position, "outbox producing position")?,
                    entry.destination(),
                    super::content::PAGED_INLINE_VALUE,
                    payload_digest.as_bytes().as_slice(),
                    payload_byte_length,
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
