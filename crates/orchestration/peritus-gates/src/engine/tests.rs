#![allow(clippy::unwrap_used, reason = "tests use fixed valid fixtures")]

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use super::GateEngine;
use crate::test_support as support;
use crate::{
    EvidencePublication, GateCommandKind, GateError, GateErrorKind, GateEvidencePublisher,
    GateEvidenceReceipt, GateRecoveryAction, GateRejection,
};

#[test]
fn exact_resolved_dispatch_never_recreates_an_effect_permit() {
    let fixture = support::fixture(1);
    let (_directory, mut journal) = journal(140);
    let (mut first, _) = GateEngine::start(
        &mut journal,
        fixture.plan.clone(),
        &support::start_command(&fixture, 140),
    )
    .expect("start");
    let attempt = support::attempt(&fixture, 141, 1);
    let prepare = support::command(
        first.state(),
        141,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    first.commit(&mut journal, &prepare).expect("prepare");

    let replay = crate::load_gate_replay(&journal, fixture.run_id).expect("load");
    let mut second = GateEngine::resume(fixture.plan.clone(), &replay).expect("resume");
    let dispatch = support::command(
        first.state(),
        142,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    second.commit(&mut journal, &dispatch).expect("initial dispatch commit");
    assert_eq!(second.effect_permit, Some(attempt.execution_id()));

    first.commit(&mut journal, &dispatch).expect("exact resolution");
    assert_eq!(first.state(), second.state());
    assert_eq!(first.effect_permit, None);
}

#[test]
fn engine_rejects_commit_through_another_store_without_mutation() {
    let fixture = support::fixture(1);
    let (_bound_directory, mut bound) = journal(210);
    let (_foreign_directory, mut foreign) = journal(211);
    let start = support::start_command(&fixture, 210);
    let (mut engine, _) =
        GateEngine::start(&mut bound, fixture.plan.clone(), &start).expect("start bound engine");
    GateEngine::start(&mut foreign, fixture.plan.clone(), &start).expect("mirror foreign head");
    assert_eq!(engine.store_id(), bound.store_id());
    assert_ne!(engine.store_id(), foreign.store_id());

    let attempt = support::attempt(&fixture, 211, 1);
    let command = support::command(
        engine.state(),
        211,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    let state_before = engine.state().clone();
    let aggregate = crate::gate_aggregate_key(fixture.run_id).expect("aggregate");
    let foreign_head_before = foreign.head(aggregate).expect("foreign head");
    let error = engine.commit(&mut foreign, &command).expect_err("foreign store commit");
    assert_eq!(error.kind(), GateErrorKind::Journal);
    assert_eq!(error.recovery(), GateRecoveryAction::CorrectInput);
    assert_eq!(engine.state(), &state_before);
    assert_eq!(engine.effect_permit, None);
    assert_eq!(foreign.head(aggregate).expect("foreign head after"), foreign_head_before);
}

#[test]
fn resolved_dispatch_after_later_checkpoint_requires_replay_and_no_effect() {
    let fixture = support::fixture(1);
    let (_directory, mut journal) = journal(150);
    let (mut stale, _) = GateEngine::start(
        &mut journal,
        fixture.plan.clone(),
        &support::start_command(&fixture, 150),
    )
    .expect("start");
    let attempt = support::attempt(&fixture, 151, 1);
    let prepare = support::command(
        stale.state(),
        151,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    stale.commit(&mut journal, &prepare).expect("prepare");

    let replay = crate::load_gate_replay(&journal, fixture.run_id).expect("load");
    let mut advancing = GateEngine::resume(fixture.plan.clone(), &replay).expect("resume");
    let dispatch = support::command(
        stale.state(),
        152,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    advancing.commit(&mut journal, &dispatch).expect("dispatch");
    let cancel = support::command(advancing.state(), 153, GateCommandKind::BeginCancellation);
    advancing.commit(&mut journal, &cancel).expect("advance checkpoint");

    let before = stale.state().clone();
    let error = stale.commit(&mut journal, &dispatch).expect_err("stale resolution");
    assert_eq!(error.recovery(), GateRecoveryAction::ReplayAggregate);
    assert_eq!(stale.state(), &before);
    assert_eq!(stale.effect_permit, None);
    let replay = crate::load_gate_replay(&journal, fixture.run_id).expect("reload");
    let resumed = GateEngine::resume(fixture.plan, &replay).expect("replay actual head");
    assert_eq!(resumed.state(), advancing.state());
    assert_eq!(resumed.effect_permit, None);
}

#[test]
fn publisher_receipt_for_another_result_position_is_rejected() {
    let fixture = support::fixture(1);
    let (_directory, mut journal) = journal(180);
    let start = support::start_command(&fixture, 180);
    let started = crate::start(&fixture.plan, &start).expect("start");
    crate::commit_gate_transition(&mut journal, &start, &started).expect("commit start");
    let mut state = started.into_state();
    let attempt = support::attempt(&fixture, 181, 1);
    commit_kind(
        &mut journal,
        &fixture,
        &mut state,
        181,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    commit_kind(
        &mut journal,
        &fixture,
        &mut state,
        182,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    let result = commit_kind(
        &mut journal,
        &fixture,
        &mut state,
        183,
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            result: support::passing(fixture.first, 183),
        },
    );
    let replay = crate::load_gate_replay(&journal, fixture.run_id).expect("load");
    let engine = GateEngine::resume(fixture.plan, &replay).expect("resume");
    let error = engine
        .publish_evidence(
            &journal,
            fixture.first,
            &result.records()[0],
            &mut WrongPositionPublisher,
        )
        .expect_err("wrong publication binding");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::EvidenceInvalid));
}

#[test]
fn same_result_event_from_another_store_is_not_authoritative_provenance() {
    let fixture = support::fixture(1);
    let padding = support::fixture_with_run(1, 16);
    let (_authoritative_directory, mut authoritative) = journal(200);
    let padding_command = support::start_command(&padding, 189);
    let padding_transition = crate::start(&padding.plan, &padding_command).expect("padding start");
    crate::commit_gate_transition(&mut authoritative, &padding_command, &padding_transition)
        .expect("commit padding event");
    let (engine, authoritative_result) =
        engine_after_passing_result(&mut authoritative, &fixture, 190);

    let (_foreign_directory, mut foreign) = journal(201);
    let (_, foreign_result) = engine_after_passing_result(&mut foreign, &fixture, 190);
    let authoritative_record = &authoritative_result.records()[0];
    let foreign_record = &foreign_result.records()[0];
    assert_eq!(authoritative_record.event_id(), foreign_record.event_id());
    assert_eq!(authoritative_record.frame_bytes(), foreign_record.frame_bytes());
    assert_ne!(authoritative_record.global_position(), foreign_record.global_position());

    let mut publisher = RecordingPublisher { called: false };
    let error = engine
        .publish_evidence(&authoritative, fixture.first, foreign_record, &mut publisher)
        .expect_err("foreign store position");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::EvidenceInvalid));
    assert!(!publisher.called);

    let error = engine
        .publish_evidence(&foreign, fixture.first, foreign_record, &mut publisher)
        .expect_err("foreign journal and record");
    assert_eq!(error.kind(), GateErrorKind::Journal);
    assert_eq!(error.recovery(), GateRecoveryAction::CorrectInput);
    assert!(!publisher.called);
}

struct WrongPositionPublisher;

impl GateEvidencePublisher for WrongPositionPublisher {
    fn publish(
        &mut self,
        publication: &EvidencePublication,
    ) -> Result<GateEvidenceReceipt, GateError> {
        let wrong = EvidencePublication::new(
            publication.run_id(),
            publication.gate_id(),
            publication.attempt(),
            publication.revision(),
            publication.result_event(),
            publication.result_position().saturating_add(1),
            publication.result_digest(),
            publication.required().to_vec(),
            publication.quality_artifacts().to_vec(),
        )?;
        wrong.receipt_from_records(Vec::new())
    }
}

struct RecordingPublisher {
    called: bool,
}

impl GateEvidencePublisher for RecordingPublisher {
    fn publish(
        &mut self,
        publication: &EvidencePublication,
    ) -> Result<GateEvidenceReceipt, GateError> {
        self.called = true;
        publication.receipt_from_records(Vec::new())
    }
}

fn engine_after_passing_result(
    journal: &mut SqliteJournal,
    fixture: &support::Fixture,
    seed: u8,
) -> (GateEngine, peritus_journal::CommittedBatch) {
    let start = support::start_command(fixture, seed);
    let started = crate::start(&fixture.plan, &start).expect("start");
    crate::commit_gate_transition(journal, &start, &started).expect("commit start");
    let mut state = started.into_state();
    let attempt = support::attempt(fixture, seed.wrapping_add(1), 1);
    commit_kind(
        journal,
        fixture,
        &mut state,
        seed.wrapping_add(1),
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    commit_kind(
        journal,
        fixture,
        &mut state,
        seed.wrapping_add(2),
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    let result = commit_kind(
        journal,
        fixture,
        &mut state,
        seed.wrapping_add(3),
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            result: support::passing(fixture.first, seed.wrapping_add(3)),
        },
    );
    let replay = crate::load_gate_replay(journal, fixture.run_id).expect("load replay");
    assert_eq!(replay.store_id(), journal.store_id());
    let engine = GateEngine::resume(fixture.plan.clone(), &replay).expect("resume");
    assert_eq!(engine.store_id(), journal.store_id());
    (engine, result)
}

fn commit_kind(
    journal: &mut SqliteJournal,
    fixture: &support::Fixture,
    state: &mut crate::GateRunState,
    seed: u8,
    kind: GateCommandKind,
) -> peritus_journal::CommittedBatch {
    let command = support::command(state, seed, kind);
    let transition = crate::decide(&fixture.plan, state, &command).expect("decide");
    let committed =
        crate::commit_gate_transition(journal, &command, &transition).expect("commit transition");
    *state = transition.into_state();
    committed
}

fn journal(seed: u8) -> (tempfile::TempDir, SqliteJournal) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let journal = SqliteJournal::open(
        directory.path().join("gate.sqlite3"),
        StoreId::new([seed; 16]).expect("store id"),
        SqliteJournalOptions::default(),
    )
    .expect("journal");
    (directory, journal)
}
