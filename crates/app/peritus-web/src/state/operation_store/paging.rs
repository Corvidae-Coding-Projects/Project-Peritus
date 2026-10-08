//! High-water keyset reads over the durable pending-operation index.

use super::{
    OperationStore, PAGE_RECORDS, RECORD_VERSION, Result, digest, problem, read_envelope,
    read_record,
};
use super::index::index_generation;
use rusqlite::{OptionalExtension, params};

impl OperationStore {
    pub(crate) fn pending_page(
        &self,
        after: Option<&str>,
        expected_snapshot: Option<&str>,
    ) -> Result<super::PendingPage> {
        if after.is_some() && expected_snapshot.is_none() {
            return Err(problem(
                "A pending-operation continuation requires its original snapshot",
            ));
        }
        let connection = self.index_connection()?;
        let generation = index_generation(&connection)?;
        let highwater = match expected_snapshot {
            Some(snapshot) => {
                decode_position(snapshot, "snapshot", &self.namespace, &generation)?
            }
            None => connection
                .query_row(
                    "SELECT coalesce(max(sequence),0) FROM pending_operations",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(problem)?,
        };
        let after = after
            .map(|cursor| decode_position(cursor, "cursor", &self.namespace, &generation))
            .transpose()?
            .unwrap_or(0);
        if after > highwater {
            return Err(problem("The pending-operation cursor exceeds its snapshot"));
        }

        let mut statement = connection
            .prepare(
                "SELECT sequence,operation,digest FROM pending_operations
                 WHERE sequence>?1 AND sequence<=?2
                 ORDER BY sequence ASC LIMIT ?3",
            )
            .map_err(problem)?;
        let rows = statement
            .query_map(
                params![after, highwater, i64::try_from(PAGE_RECORDS).map_err(problem)?],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(problem)?;
        let mut indexed = Vec::with_capacity(PAGE_RECORDS);
        for row in rows {
            indexed.push(row.map_err(problem)?);
        }
        drop(statement);

        let mut operations = Vec::with_capacity(indexed.len());
        for (_, operation, name) in &indexed {
            let path = self.root.join("pending").join(format!("{name}.json"));
            let Some(envelope) = read_envelope(&path)? else {
                if read_record(
                    &self.root.join("settled").join(format!("{name}.json")),
                    operation,
                )?
                .is_none() {
                    operations.push((operation.clone(), serde_json::json!({
                        "command":"recovery-unknown",
                        "evidence":"operation-identity-only"
                    })));
                }
                continue;
            };
            if envelope.schema_version != RECORD_VERSION {
                return Err(problem("A pending operation uses an unsupported record schema"));
            }
            if envelope.operation != *operation
                || digest(b"peritus/web/operation-record/v1\0", &envelope.operation) != *name
            {
                return Err(problem("A pending operation record has the wrong durable identity"));
            }
            if envelope.record.result.is_none() {
                operations.push((envelope.operation, envelope.record.input));
            }
        }

        let last = indexed.last().map(|(sequence, _, _)| *sequence);
        let more = match last {
            Some(last) => connection
                .query_row(
                    "SELECT 1 FROM pending_operations
                     WHERE sequence>?1 AND sequence<=?2 LIMIT 1",
                    params![last, highwater],
                    |_| Ok(()),
                )
                .optional()
                .map_err(problem)?
                .is_some(),
            None => false,
        };
        Ok(super::PendingPage {
            operations,
            cursor: more.then(|| {
                encode_position(
                    &self.namespace,
                    &generation,
                    last.expect("a continuing page has a row"),
                )
            }),
            snapshot: encode_position(&self.namespace, &generation, highwater),
        })
    }
}

fn encode_position(namespace: &str, generation: &str, sequence: i64) -> String {
    format!("v2:{namespace}:{generation}:{sequence}")
}

fn decode_position(value: &str, kind: &str, namespace: &str, generation: &str) -> Result<i64> {
    let encoded = value
        .strip_prefix("v2:")
        .ok_or_else(|| problem(format!("The pending-operation {kind} has an unsupported schema")))?;
    let (retained_namespace, encoded) = encoded
        .split_once(':')
        .ok_or_else(|| problem(format!("The pending-operation {kind} is malformed")))?;
    if retained_namespace != namespace {
        return Err(problem(format!(
            "The pending-operation {kind} belongs to another durable workspace"
        )));
    }
    let (retained_generation, encoded) = encoded
        .split_once(':')
        .ok_or_else(|| problem(format!("The pending-operation {kind} is malformed")))?;
    if retained_generation != generation {
        return Err(problem(format!(
            "The pending-operation {kind} belongs to an obsolete index generation"
        )));
    }
    let sequence = encoded.parse::<i64>().map_err(problem)?;
    if sequence < 0 || encode_position(namespace, generation, sequence) != value {
        return Err(problem(format!("The pending-operation {kind} is malformed")));
    }
    Ok(sequence)
}
