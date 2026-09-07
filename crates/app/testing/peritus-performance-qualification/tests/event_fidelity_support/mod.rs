//! Independent retained-evidence and actual `SQLite` byte audit, outside the timed adapter.

use peritus_benchmarks::{PlannedOperation, QualificationPlan, Sha256Digest};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{BufRead as _, BufReader};
use std::path::Path;

pub fn open(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("existing read-only SQLite snapshot")
}

pub fn read_rows(path: &Path) -> Vec<Value> {
    BufReader::new(std::fs::File::open(path).expect("raw file"))
        .lines()
        .map(|line| serde_json::from_str(&line.expect("raw line")).expect("raw JSON"))
        .collect()
}

pub fn verify(
    rows: &[Value],
    database: &rusqlite::Connection,
    plan: &QualificationPlan,
    initial: &[Value],
) -> Result<u64, String> {
    let expected = plan.step_count();
    let mut arrivals = 0;
    let mut accepted = BTreeSet::new();
    let mut commits = 0;
    let mut commands = 0;
    let mut offered = 0;
    let mut missed = 0;
    let mut full = 0;
    let mut expired = 0;
    let mut failures = 0;
    let mut event_ids = BTreeSet::new();
    let mut query = database
        .prepare("SELECT frame FROM events WHERE event_id=?1")
        .map_err(|error| error.to_string())?;
    for row in rows {
        match row["record"].as_str() {
            Some("arrival") => {
                verify_plan_step(row, plan)?;
                let sequence = number(row, "sequence")?;
                require(sequence == arrivals, "noncontiguous arrival")?;
                arrivals += 1;
                require(
                    number(row, "observed_us")? >= number(row, "scheduled_us")?,
                    "early arrival",
                )?;
                match row["outcome"].as_str() {
                    Some("schedule_missed") => missed += 1,
                    Some("queue_full") => {
                        offered += 1;
                        full += 1;
                    }
                    Some("queued" | "dispatched") => {
                        offered += 1;
                        accepted.insert(sequence);
                    }
                    _ => return Err("unknown arrival outcome".to_owned()),
                }
            }
            Some("expired") => {
                require(accepted.remove(&number(row, "sequence")?), "expiry without admission")?;
                expired += 1;
            }
            Some("completion") => {
                verify_plan_step(row, plan)?;
                require(
                    accepted.remove(&number(row, "sequence")?),
                    "completion without unique admission",
                )?;
                require(
                    number(row, "finished_us")? >= number(row, "started_us")?
                        && number(row, "started_us")? >= number(row, "scheduled_us")?,
                    "invalid completion times",
                )?;
                commands += number(row, "committed_commands")?;
                if !row["failure"].is_null() {
                    failures += 1;
                    continue;
                }
                require(number(row, "committed_commands")? == 3, "missing prerequisite commits")?;
                let id: [u8; 16] = serde_json::from_value(row["event_id"].clone())
                    .map_err(|error| error.to_string())?;
                require(event_ids.insert(id), "duplicate committed event")?;
                verify_frame(row, &mut query, id)?;
                commits += 1;
            }
            Some("summary") => {}
            _ => return Err("unknown evidence record".to_owned()),
        }
    }
    require(
        arrivals == expected && accepted.is_empty(),
        "incomplete arrival or terminal coverage",
    )?;
    let terminal = rows.last().ok_or("missing summary")?;
    require(terminal["record"] == "summary", "missing terminal summary")?;
    let summary = &terminal["counters"];
    for (key, value) in [
        ("expected", expected),
        ("offered", offered),
        ("schedule_missed", missed),
        ("queue_full", full),
        ("expired", expired),
        ("committed", commits),
        ("failed", failures),
        ("committed_commands", commands),
    ] {
        require(number(summary, key)? == value, "summary disagrees with unsampled observations")?;
    }
    let stored: i64 = database
        .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    verify_initial(initial, &mut query)?;
    if u64::try_from(stored).ok()
        != Some(commands + u64::try_from(initial.len()).map_err(|error| error.to_string())?)
    {
        return Err(format!(
            "actual total committed event count {stored} differs from reported {commands}"
        ));
    }
    Ok(commits)
}

pub fn initial_events(database: &rusqlite::Connection) -> Vec<Value> {
    database
        .prepare("SELECT event_id,frame FROM events ORDER BY global_position")
        .expect("initial query")
        .query_map([], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)))
        .expect("initial rows")
        .map(|row| {
            let (id, frame) = row.expect("initial event");
            serde_json::json!({"event_id": id, "digest": Sha256Digest::of_bytes(&frame)})
        })
        .collect()
}

fn verify_initial(initial: &[Value], query: &mut rusqlite::Statement<'_>) -> Result<(), String> {
    for entry in initial {
        let id: [u8; 16] =
            serde_json::from_value(entry["event_id"].clone()).map_err(|error| error.to_string())?;
        let frame: Vec<u8> = query
            .query_row([id.as_slice()], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        require(
            entry["digest"] == Sha256Digest::of_bytes(&frame).to_string(),
            "startup event changed",
        )?;
    }
    Ok(())
}

fn verify_plan_step(row: &Value, plan: &QualificationPlan) -> Result<(), String> {
    let step = plan.step(number(row, "sequence")?).ok_or("unexpected sequence")?;
    let PlannedOperation::AppendEvent { bytes } = step.operation() else {
        return Err("unexpected operation".to_owned());
    };
    require(
        number(row, "scheduled_us")? == step.offset_micros()
            && number(row, "payload_bytes")? == u64::from(*bytes),
        "observed schedule or payload differs from source plan",
    )
}

fn verify_frame(
    row: &Value,
    query: &mut rusqlite::Statement<'_>,
    id: [u8; 16],
) -> Result<(), String> {
    let frame: Vec<u8> =
        query.query_row([id.as_slice()], |row| row.get(0)).map_err(|error| error.to_string())?;
    let payload =
        usize::try_from(number(row, "payload_bytes")?).map_err(|error| error.to_string())?;
    require(
        frame.len() == payload + peritus_codec::HEADER_LEN,
        "stored payload length differs from plan",
    )?;
    require(
        row["expected_event_digest"] == Sha256Digest::of_bytes(&frame).to_string(),
        "stored event bytes differ from submitted event",
    )?;
    let event = peritus_codec::decode_message::<peritus_review::ReviewEventFrame>(
        &frame,
        peritus_codec::CodecLimits::PRODUCTION,
    )
    .map_err(|error| error.to_string())?
    .into_event();
    require(
        matches!(event.kind(), peritus_review::ReviewEventKind::ReviewSubmitted { .. }),
        "not an actual review-submission event",
    )
}

fn number(value: &Value, key: &str) -> Result<u64, String> {
    value[key].as_u64().ok_or_else(|| format!("missing numeric {key}"))
}
fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition { Ok(()) } else { Err(message.to_owned()) }
}
