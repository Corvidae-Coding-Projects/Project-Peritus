//! Real-SQLite D2 commit, restart, idempotency, conflict, and corruption tests.

#![allow(clippy::unwrap_used, reason = "fixed durability fixtures use checked values")]

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_review::{
    ReviewBinding, ReviewCommand, ReviewCommandFrame, ReviewCommandKind, ReviewErrorKind,
    ReviewLimits, ReviewRunPhase, commit_review_transition, decide, load_review_replay, start,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EnvironmentId, EventId, GateId, Generation, HarnessId,
    PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

#[test]
fn durability_restart_idempotency_conflict_and_checkpoint_corruption() {
    let fixture = Fixture::new();
    let binding = fixture.binding(90);
    let command = fixture.genesis(binding, 1, 2);
    let transition = start(&command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("review.sqlite3");
    let store_id = StoreId::new(bytes(80)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();

    let first = commit_review_transition(&mut journal, &command, &transition).unwrap();
    let resolved = commit_review_transition(&mut journal, &command, &transition).unwrap();
    assert_eq!(first.batch_hash(), resolved.batch_hash());
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let replay = load_review_replay(&restarted, command.run_id()).unwrap();
    assert_eq!(replay.rebuild().unwrap(), Some(transition.state().clone()));

    let conflict = fixture.genesis(fixture.binding(91), 1, 3);
    let conflict_transition = start(&conflict).unwrap();
    let error = commit_review_transition(&mut restarted, &conflict, &conflict_transition)
        .expect_err("one command identity cannot name different canonical bytes");
    assert_eq!(error.kind(), ReviewErrorKind::Journal);
    drop(restarted);

    corrupt_family_55_checkpoint(&path);
    let corrupted = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    assert!(load_review_replay(&corrupted, command.run_id()).is_err());
}

#[test]
fn pause_resume_is_canonical_durable_idempotent_and_authority_neutral() {
    let fixture = Fixture::new();
    let start_command = fixture.genesis(fixture.binding(92), 10, 11);
    let started = start(&start_command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("review-pause.sqlite3");
    let store_id = StoreId::new(bytes(82)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_review_transition(&mut journal, &start_command, &started).unwrap();
    let active = started.into_state();

    let pause_command = fenced(&active, 12, 13, ReviewCommandKind::PauseRun);
    let paused = decide(&active, &pause_command).unwrap();
    assert_eq!(paused.state().phase(), ReviewRunPhase::Paused);
    assert_eq!(paused.state().limits(), active.limits());
    assert_eq!(paused.state().binding(), active.binding());
    assert_eq!(paused.state().findings(), active.findings());
    assert_eq!(paused.state().quorum(), active.quorum());
    assert!(paused.state().terminal().is_none());

    let bytes =
        encode_message(&ReviewCommandFrame::from_command(&pause_command), CodecLimits::PRODUCTION)
            .unwrap();
    assert_eq!(
        decode_message::<ReviewCommandFrame>(&bytes, CodecLimits::PRODUCTION).unwrap().0,
        pause_command
    );
    let finalize = fenced(paused.state(), 14, 15, ReviewCommandKind::FinalizeRun);
    assert_eq!(
        decide(paused.state(), &finalize).unwrap_err().kind(),
        ReviewErrorKind::IllegalTransition
    );

    let first = commit_review_transition(&mut journal, &pause_command, &paused).unwrap();
    let resolved = commit_review_transition(&mut journal, &pause_command, &paused).unwrap();
    assert_eq!(first.batch_hash(), resolved.batch_hash());
    let expected_paused = paused.into_state();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let replayed_paused =
        load_review_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(replayed_paused, expected_paused);
    let resume_command = fenced(&replayed_paused, 16, 17, ReviewCommandKind::ResumeRun);
    let resumed = decide(&replayed_paused, &resume_command).unwrap();
    assert_eq!(resumed.state().phase(), ReviewRunPhase::Active);
    assert_eq!(resumed.state().limits(), active.limits());
    assert_eq!(resumed.state().findings(), active.findings());
    assert_eq!(resumed.state().quorum(), active.quorum());
    assert!(resumed.state().terminal().is_none());
    commit_review_transition(&mut restarted, &resume_command, &resumed).unwrap();
    let expected_resumed = resumed.into_state();
    drop(restarted);

    let restarted = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    assert_eq!(
        load_review_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap(),
        Some(expected_resumed)
    );
}

#[test]
fn absent_ahead_and_behind_checkpoints_fail_closed() {
    for mutation in
        [CheckpointMutation::Absent, CheckpointMutation::Ahead, CheckpointMutation::Behind]
    {
        let (directory, path, store_id, run_id) = committed_two_event_store();
        mutate_checkpoint(&path, mutation);
        let journal =
            SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
        assert!(load_review_replay(&journal, run_id).is_err(), "{mutation:?} must fail closed");
        drop(journal);
        drop(directory);
    }
}

#[derive(Clone, Copy, Debug)]
enum CheckpointMutation {
    Absent,
    Ahead,
    Behind,
}

fn committed_two_event_store() -> (tempfile::TempDir, std::path::PathBuf, StoreId, RunId) {
    let fixture = Fixture::new();
    let command = fixture.genesis(fixture.binding(90), 1, 2);
    let transition = start(&command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("review.sqlite3");
    let store_id = StoreId::new(bytes(81)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_review_transition(&mut journal, &command, &transition).unwrap();
    let state = transition.into_state();
    let failure = ReviewCommand::new(
        CommandId::new(bytes(3)).unwrap(),
        EventId::new(bytes(4)).unwrap(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        ReviewCommandKind::FailRun { failure_digest: digest(82) },
    )
    .unwrap();
    let failed = decide(&state, &failure).unwrap();
    commit_review_transition(&mut journal, &failure, &failed).unwrap();
    drop(journal);
    (directory, path, store_id, command.run_id())
}

fn mutate_checkpoint(path: &std::path::Path, mutation: CheckpointMutation) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let changed = match mutation {
        CheckpointMutation::Absent => connection.execute(
            "DELETE FROM state_records WHERE namespace = ?1",
            [i64::from(peritus_review::REVIEW_STATE_NAMESPACE)],
        ),
        CheckpointMutation::Ahead => connection.execute(
            "UPDATE state_records SET revision = 3 WHERE namespace = ?1",
            [i64::from(peritus_review::REVIEW_STATE_NAMESPACE)],
        ),
        CheckpointMutation::Behind => connection.execute(
            "UPDATE state_records SET revision = 1 WHERE namespace = ?1",
            [i64::from(peritus_review::REVIEW_STATE_NAMESPACE)],
        ),
    }
    .unwrap();
    assert_eq!(changed, 1);
}

fn corrupt_family_55_checkpoint(path: &std::path::Path) {
    let mut bytes = std::fs::read(path).unwrap();
    let marker = b"PRTS\x00\x01\x00\x37\x00\x01\x00\x00";
    let starts = bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(index, window)| (window == marker).then_some(index))
        .collect::<Vec<_>>();
    assert!(!starts.is_empty(), "family-55 bytes must exist in the SQLite database");
    for start in starts {
        let payload_len = u32::from_be_bytes(bytes[start + 12..start + 16].try_into().unwrap());
        let end = start + 16 + payload_len as usize;
        assert!(end <= bytes.len() && payload_len > 0);
        bytes[end - 1] ^= 0x01;
    }
    std::fs::write(path, bytes).unwrap();
}

struct Fixture {
    contract: AcceptanceContract,
    revision: RevisionTuple,
    limits: ReviewLimits,
    producer: ActorId,
}

impl Fixture {
    fn new() -> Self {
        let acceptance = AcceptanceSpecId::new(bytes(10)).unwrap();
        let category = ReviewCategory::new(digest(11));
        let gate_id = GateId::new(bytes(12)).unwrap();
        let gate_evidence = EvidenceRequirementId::new(digest(13));
        let review_evidence = EvidenceRequirementId::new(digest(14));
        let gate = GateDefinition::new(
            gate_id,
            GateExecutionPlan::new(
                content(15),
                EnvironmentId::new(bytes(16)).unwrap(),
                content(17),
                content(18),
                GateSuccessRule::ExitCodeZero,
                1_000,
                content(19),
                GateFreshnessScope::ExactRevisionTuple,
            )
            .unwrap(),
            Vec::new(),
            vec![gate_evidence],
        )
        .unwrap();
        let revision = RevisionTuple::new(
            acceptance,
            HarnessId::new(bytes(20)).unwrap(),
            WorkspaceId::new(bytes(21)).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(bytes(22)).unwrap(),
            ProviderProfileId::new(bytes(23)).unwrap(),
        );
        let contract = AcceptanceContract::new(
            acceptance,
            digest(24),
            ContractDocuments::new(
                content(25),
                content(26),
                content(27),
                content(28),
                content(29),
                content(30),
                content(31),
                content(32),
            ),
            vec![Requirement::new(RequirementId::new(digest(33)), content(34))],
            vec![Exclusion::new(content(35))],
            vec![Assumption::new(content(36))],
            GateGraph::new(vec![gate]).unwrap(),
            ReviewPolicy::new(
                vec![category],
                1,
                ReviewerIndependence::new(true, true, true, true, true, true),
                FindingSeverity::High,
            )
            .unwrap(),
            vec![
                EvidenceRequirement::new(
                    gate_evidence,
                    content(37),
                    EvidenceSource::Gate(gate_id),
                    ExportClassification::Internal,
                ),
                EvidenceRequirement::new(
                    review_evidence,
                    content(38),
                    EvidenceSource::Review(category),
                    ExportClassification::Internal,
                ),
            ],
            CompletionPolicy::new(2, 4).unwrap(),
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        )
        .unwrap();
        Self { contract, revision, limits: limits(), producer: ActorId::new(bytes(39)).unwrap() }
    }

    fn binding(&self, candidate: u8) -> ReviewBinding {
        ReviewBinding::from_contract(
            &self.contract,
            self.revision,
            digest(candidate),
            digest(candidate.wrapping_add(1)),
            vec![self.producer],
            vec![digest(40)],
            self.limits,
        )
        .unwrap()
    }

    fn genesis(&self, binding: ReviewBinding, command: u8, event: u8) -> ReviewCommand {
        ReviewCommand::new(
            CommandId::new(bytes(command)).unwrap(),
            EventId::new(bytes(event)).unwrap(),
            RunId::new(bytes(70)).unwrap(),
            0,
            None,
            digest(0),
            binding.revision(),
            ReviewCommandKind::StartRun { binding, limits: self.limits },
        )
        .unwrap()
    }
}

fn fenced(
    state: &peritus_review::ReviewRunState,
    command: u8,
    event: u8,
    kind: ReviewCommandKind,
) -> ReviewCommand {
    ReviewCommand::new(
        CommandId::new(bytes(command)).unwrap(),
        EventId::new(bytes(event)).unwrap(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        kind,
    )
    .unwrap()
}

fn limits() -> ReviewLimits {
    ReviewLimits::new(
        16, 16, 16, 128, 16, 16, 16, 32, 16, 32, 256, 4_096, 4_096, 1_048_576, 4_194_304,
    )
    .unwrap()
}

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
