//! Deterministic real-SQLite regression for a final in-flight outbox delivery attempt.
//!
//! Run: `cargo run --locked -p peritus-journal --example outbox_final_claim`.

use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, JournalError, JournalErrorKind, OutboxDraft, OutboxId, OutboxState,
    SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn checked<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
}

struct Fixture {
    journal: SqliteJournal,
    first: OutboxId,
    second: OutboxId,
    _directory: Directory,
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn temporary() -> Result<Directory, String> {
    let stamp = checked(SystemTime::now().duration_since(UNIX_EPOCH))?.as_nanos();
    for attempt in 0..32 {
        let path = std::env::temp_dir()
            .join(format!("peritus-outbox-final-{}-{stamp}-{attempt}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(Directory(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("could not reserve a disposable regression directory".into())
}

fn fixture(maximum: u16, include_second: bool) -> Result<Fixture, String> {
    let directory = temporary()?;
    let mut journal = checked(SqliteJournal::open(
        directory.0.join("journal.sqlite"),
        checked(StoreId::new([41; 16]))?,
        SqliteJournalOptions::default(),
    ))?;
    let aggregate = AggregateKey::new(AggregateKind::Trace, checked(AggregateId::new([1; 16]))?);
    let frame = checked(ExactFrame::new(checked(peritus_codec::encode_frame(
        1,
        1,
        b"exact synthetic outbox producer",
        peritus_codec::CodecLimits::PRODUCTION,
    ))?))?;
    let event = checked(EventDraft::new(
        aggregate,
        checked(EventSequence::new(1))?,
        checked(EventId::new([2; 16]))?,
        None,
        frame,
        Sha256Digest::new([3; 32]),
        vec![],
    ))?;
    let first = checked(OutboxId::new([4; 16]))?;
    let second = checked(OutboxId::new([7; 16]))?;
    let mut outbox = vec![checked(OutboxDraft::new(
        first,
        "fixture://first".into(),
        b"first exact delivery".to_vec(),
        maximum,
    ))?];
    if include_second {
        outbox.push(checked(OutboxDraft::new(
            second,
            "fixture://second".into(),
            b"unrelated due delivery".to_vec(),
            1,
        ))?);
    }
    let plan = checked(
        AppendRequest::new(
            journal.store_id(),
            checked(CommandId::new([5; 16]))?,
            Sha256Digest::new([6; 32]),
            vec![HeadExpectation::Absent(aggregate)],
            vec![event],
            vec![],
            vec![],
            None,
            None,
            outbox,
        )
        .plan(),
    )?;
    checked(journal.append(plan))?;
    Ok(Fixture { journal, first, second, _directory: directory })
}

fn require_error(
    result: Result<(), JournalError>,
    expected: JournalErrorKind,
) -> Result<(), String> {
    match result {
        Err(error) if error.kind() == expected => Ok(()),
        other => Err(format!("expected {expected:?}; observed {other:?}")),
    }
}

fn final_claim(maximum: u16, competing_tick: u64) -> Result<(), String> {
    let mut fixture = fixture(maximum, false)?;
    for attempt in 1..maximum {
        let claim =
            checked(fixture.journal.claim_outbox(u64::from(attempt), u64::from(attempt) + 1))?
                .ok_or("earlier claim missing")?;
        if claim.attempts() != attempt {
            return Err("earlier attempt count differs".into());
        }
    }
    let claim = checked(fixture.journal.claim_outbox(u64::from(maximum), 11))?
        .ok_or("final claim missing")?;
    let fence = claim.fence().ok_or("final claim fence missing")?;
    if claim.attempts() != maximum || claim.lease_until() != Some(11) {
        return Err("final permitted claim was not established".into());
    }
    if checked(fixture.journal.claim_outbox(competing_tick, competing_tick + 1))?.is_some() {
        return Err("final attempt was redispatched".into());
    }
    let after =
        checked(fixture.journal.outbox_message(fixture.first))?.ok_or("delivery missing")?;
    if competing_tick < 11 {
        if after.state() != OutboxState::Claimed
            || after.fence() != Some(fence)
            || after.lease_until() != Some(11)
            || after.attempts() != maximum
        {
            return Err(format!("competing claim revoked a live final attempt: {after:?}"));
        }
        require_error(
            fixture.journal.acknowledge_outbox(fixture.first, fence + 1),
            JournalErrorKind::StaleHead,
        )?;
        checked(fixture.journal.acknowledge_outbox(fixture.first, fence))?;
        // Already acknowledged delivery is idempotent for every positive fence by this API.
        checked(fixture.journal.acknowledge_outbox(fixture.first, fence + 1))?;
        require_error(
            fixture.journal.acknowledge_outbox(fixture.first, 0),
            JournalErrorKind::InvalidInput,
        )?;
        if checked(fixture.journal.outbox_message(fixture.first))?
            .ok_or("acknowledged row missing")?
            .state()
            != OutboxState::Acknowledged
        {
            return Err("acknowledged state was not retained".into());
        }
    } else {
        if after.state() != OutboxState::Exhausted
            || after.lease_until().is_some()
            || after.fence() != Some(fence)
            || after.attempts() != maximum
        {
            return Err(format!("expired final attempt was not exhausted exactly once: {after:?}"));
        }
        require_error(
            fixture.journal.acknowledge_outbox(fixture.first, fence),
            JournalErrorKind::StaleHead,
        )?;
    }
    Ok(())
}

fn unrelated_due_row() -> Result<(), String> {
    let mut fixture = fixture(1, true)?;
    let first = checked(fixture.journal.claim_outbox(1, 11))?.ok_or("first claim absent")?;
    let second =
        checked(fixture.journal.claim_outbox(2, 3))?.ok_or("unrelated due row was blocked")?;
    if first.id() != fixture.first || second.id() != fixture.second {
        return Err("claim ordering changed".into());
    }
    checked(
        fixture.journal.acknowledge_outbox(fixture.first, first.fence().ok_or("first fence")?),
    )?;
    checked(
        fixture.journal.acknowledge_outbox(fixture.second, second.fence().ok_or("second fence")?),
    )?;
    checked(fixture.journal.integrity_scan())?;
    Ok(())
}

fn main() -> Result<(), String> {
    for maximum in [1, 2] {
        for tick in [10, 11, 12] {
            final_claim(maximum, tick)?;
        }
    }
    unrelated_due_row()?;
    println!(
        "Live final claims survive competing polls; expiry boundaries, multiple attempts, unrelated due work, stale fences, and repeated ACKs preserve their contracts."
    );
    Ok(())
}
