//! Readable archive descriptors with a stable scoped snapshot cursor and exact byte bounds.

use super::{LocalMemory, error, hex, rejected, source_handle};
use peritus_agent::DeveloperLoopError;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    scope: String,
    #[serde(deserialize_with = "required_cursor")]
    cursor: Option<String>,
    max_bytes: usize,
}

fn required_cursor<'de, D: serde::Deserializer<'de>>(value: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(value)
}

pub(super) fn execute(memory: &LocalMemory, bytes: &[u8]) -> Result<Value, DeveloperLoopError> {
    let Ok(request) = serde_json::from_slice::<Request>(bytes) else {
        return Ok(rejected(memory, "invalid source-index schema"));
    };
    let scope = hex(memory.store.scope_digest().as_bytes());
    if request.scope != scope || !(256..=memory.config.max_read_bytes).contains(&request.max_bytes)
    {
        return Ok(rejected(memory, "foreign source-index scope or invalid response allocation"));
    }
    let available =
        u64::try_from(memory.sources.len()).map_err(|_| error("source index size overflow"))?;
    let (mut next, through) = match request.cursor {
        None => (1, available),
        Some(cursor) => {
            let fields = cursor.split(':').collect::<Vec<_>>();
            let parsed = if fields.len() == 4 && fields[0] == "sources" && fields[1] == scope {
                fields[2].parse::<u64>().ok().zip(fields[3].parse::<u64>().ok())
            } else {
                None
            };
            let Some((next, through)) = parsed else {
                return Ok(rejected(memory, "invalid source-index cursor"));
            };
            if next == 0
                || through > available
                || next > through.saturating_add(1)
                || cursor != format!("sources:{scope}:{next}:{through}")
            {
                return Ok(rejected(memory, "stale, foreign or noncanonical source-index cursor"));
            }
            (next, through)
        }
    };
    let mut rows = Vec::new();
    while next <= through {
        let id = next;
        next = next.checked_add(1).ok_or_else(|| error("source-index ordinal overflow"))?;
        if !super::super::source_policy::allowed(memory, id)? {
            continue;
        }
        let source = memory.archived(id)?;
        let mut item = json!({"handle":source_handle(memory,id),"kind":source.kind,
            "tool":source.call.as_ref().map(|call|call.name.as_str()),"invocation":source.invocation,
            "sha256":hex(source.artifact.digest.as_bytes()),"bytes":source.artifact.bytes,"is_error":source.is_error});
        if source.kind == super::super::record::ArchiveKind::HostObservation
            && super::super::operation_plan::decode_message(&memory.artifact(id)?)?.is_some()
            && let super::super::operation_plan::PlanRecord::Step { output, .. } =
                super::super::operation_plan::load_record(memory, id)?
        {
            item["result"] = json!({"tool":"inspection_result_read","sha256":hex(output.digest.as_bytes()),"bytes":output.bytes});
        }
        rows.push(item);
        if response(&scope, &rows, next, through).to_string().len() > request.max_bytes {
            rows.pop();
            next = id;
            if rows.is_empty() {
                return Ok(rejected(
                    memory,
                    "max_bytes cannot fit one source descriptor; increase it",
                ));
            }
            break;
        }
    }
    Ok(response(&scope, &rows, next, through))
}

fn response(scope: &str, rows: &[Value], next: u64, through: u64) -> Value {
    json!({"authority":"none","scope":scope,"through_observation":through,"sources":rows,
        "cursor":if next<=through {Value::from(format!("sources:{scope}:{next}:{through}"))} else {Value::Null}})
}
