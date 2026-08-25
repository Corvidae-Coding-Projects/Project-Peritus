//! Immutable family-76/77/78 compatibility corpus.

mod accepted;
mod observation_binding;

pub use accepted::certificate;

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CanonicalDecode, CodecLimits, decode_message, encode_message, sha256};
use peritus_collaboration::{CollaborationId, CollaborationTaskId};
use peritus_policy::ActorRole;
use peritus_role::HarnessRole;
use peritus_scheduler::{SchedulerId, WorkId};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, AttemptId, CommandId, EnvironmentId, EventId, GateId, Generation,
    HarnessId, PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest,
    SnapshotId, TurnId, WorkspaceId,
};

use super::{OrchestratorCommandFrame, OrchestratorEventFrame, OrchestratorStateFrame};
use crate::{
    CandidateBinding, Handoff, HandoffId, HandoffKind, OrchestratorBinding, OrchestratorCommand,
    OrchestratorCommandKind, OrchestratorId, OrchestratorLimits, RoleAssignment, RoleOwnership,
    start,
};

const COMMAND_TAG_OFFSET: usize = 201;
const EVENT_TAG_OFFSET: usize = 233;

#[test]
fn canonical_corpus_is_exact_and_rejects_corruption() {
    let (command, event, state) = values();
    let corpus = [
        Fixture::new(
            "orchestrator-command.bin",
            encode_message(
                &OrchestratorCommandFrame::from_command(&command),
                CodecLimits::PRODUCTION,
            )
            .unwrap(),
        ),
        Fixture::new(
            "orchestrator-event.bin",
            encode_message(&OrchestratorEventFrame::from_event(&event), CodecLimits::PRODUCTION)
                .unwrap(),
        ),
        Fixture::new(
            "orchestrator-state.bin",
            encode_message(&OrchestratorStateFrame::from_state(&state), CodecLimits::PRODUCTION)
                .unwrap(),
        ),
    ];
    let root = fixture_root();
    let manifest = manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_ORCHESTRATOR_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &manifest);
    }

    for fixture in &corpus {
        assert_eq!(fs::read(root.join(fixture.name)).unwrap(), fixture.bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);
    assert_eq!(
        decode_message::<OrchestratorCommandFrame>(&corpus[0].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        command
    );
    assert_eq!(
        decode_message::<OrchestratorEventFrame>(&corpus[1].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        event
    );
    assert!(
        decode_message::<OrchestratorStateFrame>(&corpus[2].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .matches_state(&state)
    );
    reject_trailing::<OrchestratorCommandFrame>(&corpus[0].bytes);
    reject_trailing::<OrchestratorEventFrame>(&corpus[1].bytes);
    reject_trailing::<OrchestratorStateFrame>(&corpus[2].bytes);
    reject_tag::<OrchestratorCommandFrame>(&corpus[0].bytes, COMMAND_TAG_OFFSET);
    reject_tag::<OrchestratorEventFrame>(&corpus[1].bytes, EVENT_TAG_OFFSET);
}

pub fn values() -> (OrchestratorCommand, crate::OrchestratorEvent, crate::OrchestratorState) {
    let contract = contract();
    let revision = revision();
    let limits = limits();
    let service = actor(40);
    let writer = actor(41);
    let fixer = actor(42);
    let reviewer = actor(43);
    let ownership = RoleOwnership::new(
        service,
        ActorRole::Orchestrator,
        RoleAssignment::new(writer, ActorRole::Writer, HarnessRole::Writer).unwrap(),
        RoleAssignment::new(fixer, ActorRole::Fixer, HarnessRole::Fixer).unwrap(),
        vec![RoleAssignment::new(reviewer, ActorRole::Reviewer, HarnessRole::Reviewer).unwrap()],
        limits,
    )
    .unwrap();
    let candidate = CandidateBinding::new(
        revision,
        SnapshotId::new(bytes(44)).unwrap(),
        digest(45),
        digest(46),
        digest(47),
        None,
        None,
        vec![writer],
        vec![digest(48)],
        limits,
    )
    .unwrap();
    let handoff = Handoff::new(
        HandoffId::new(bytes(49)).unwrap(),
        HandoffKind::Writer,
        service,
        writer,
        candidate.clone(),
        Some(TurnId::new(bytes(50)).unwrap()),
        CollaborationTaskId::new(bytes(51)).unwrap(),
        WorkId::new(bytes(52)).unwrap(),
        vec![digest(53)],
        vec![digest(54)],
        Vec::new(),
        limits,
    )
    .unwrap();
    let run = RunId::new(bytes(55)).unwrap();
    let binding = OrchestratorBinding::from_contract(
        &contract,
        OrchestratorId::new(bytes(56)).unwrap(),
        run,
        AttemptId::new(bytes(57)).unwrap(),
        revision,
        RunId::new(bytes(58)).unwrap(),
        RunId::new(bytes(59)).unwrap(),
        RunId::new(bytes(60)).unwrap(),
        digest(61),
        digest(62),
        SchedulerId::new(bytes(63)).unwrap(),
        digest(64),
        CollaborationId::new(bytes(65)).unwrap(),
        digest(66),
        limits,
    )
    .unwrap();
    let command = OrchestratorCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        run,
        0,
        None,
        digest(0),
        revision,
        OrchestratorCommandKind::Start {
            genesis: Box::new(crate::OrchestratorGenesis::new(
                binding, candidate, ownership, handoff,
            )),
        },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    (command, transition.event().clone(), transition.into_state())
}

fn contract() -> AcceptanceContract {
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
    AcceptanceContract::new(
        acceptance,
        digest(20),
        ContractDocuments::new(
            content(21),
            content(22),
            content(23),
            content(24),
            content(25),
            content(26),
            content(27),
            content(28),
        ),
        vec![Requirement::new(RequirementId::new(digest(29)), content(30))],
        vec![Exclusion::new(content(31))],
        vec![Assumption::new(content(32))],
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
                content(33),
                EvidenceSource::Gate(gate_id),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                review_evidence,
                content(34),
                EvidenceSource::Review(category),
                ExportClassification::Internal,
            ),
        ],
        CompletionPolicy::new(2, 4).unwrap(),
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    )
    .unwrap()
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(10)).unwrap(),
        HarnessId::new(bytes(35)).unwrap(),
        WorkspaceId::new(bytes(36)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(37)).unwrap(),
        ProviderProfileId::new(bytes(38)).unwrap(),
    )
}

fn limits() -> OrchestratorLimits {
    OrchestratorLimits::new(8, 4, 12, 16, 20, 32, 48, 64, 96, 128, 1_048_576, 2_097_152).unwrap()
}

fn reject_tag<T: CanonicalDecode>(bytes: &[u8], offset: usize) {
    let mut corrupt = bytes.to_vec();
    corrupt[offset] = u8::MAX;
    assert!(decode_message::<T>(&corrupt, CodecLimits::PRODUCTION).is_err());
}

fn reject_trailing<T: CanonicalDecode>(bytes: &[u8]) {
    let mut corrupt = bytes.to_vec();
    corrupt.push(0);
    assert!(decode_message::<T>(&corrupt, CodecLimits::PRODUCTION).is_err());
}

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
}

impl Fixture {
    const fn new(name: &'static str, bytes: Vec<u8>) -> Self {
        Self { name, bytes }
    }
}

fn manifest(corpus: &[Fixture]) -> String {
    let mut output = String::from("# peritus orchestrator protocol compatibility corpus v1\n");
    for fixture in corpus {
        output.push_str(&hex(sha256(&fixture.bytes).as_bytes()));
        output.push_str("  ");
        output.push_str(fixture.name);
        output.push('\n');
    }
    output
}

fn write_corpus(root: &Path, corpus: &[Fixture], manifest: &str) {
    fs::create_dir_all(root).unwrap();
    for fixture in corpus {
        fs::write(root.join(fixture.name), &fixture.bytes).unwrap();
    }
    fs::write(root.join("SHA256SUMS"), manifest).unwrap();
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn actor(value: u8) -> ActorId {
    ActorId::new(bytes(value)).unwrap()
}
const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
