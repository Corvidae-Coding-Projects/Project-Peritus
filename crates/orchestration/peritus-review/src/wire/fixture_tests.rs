//! Immutable family-53/54/55 compatibility corpus.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
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

use super::{ReviewCommandFrame, ReviewEventFrame, ReviewStateFrame};
use crate::{ReviewBinding, ReviewCommand, ReviewCommandKind, ReviewLimits, ReviewRunState, start};

const COMMAND_BYTES: usize = 597;
const EVENT_BYTES: usize = 629;
const STATE_BYTES: usize = 540;
const COMMAND_TAG_OFFSET: usize = 201;
const EVENT_TAG_OFFSET: usize = 233;
const STATE_PHASE_OFFSET: usize = 427;
const COMMAND_SHA256: &str = "5698774980c67550140f31c649851ada0d9c043454943b4638dd9e6ca3c62789";
const EVENT_SHA256: &str = "c0fdc5fb09b50e493f4cbb510ae58c9992f3b805b4f0e3b63506d7bdc63f5f87";
const STATE_SHA256: &str = "7c8cb85f106fb62153d57e84408c526543af873a21571e535726bd9aa0d81388";

#[test]
fn canonical_binary_corpus_is_exact_and_rejects_closed_tag_or_trailing_corruption() {
    let (command, event, state) = values();
    let command_bytes =
        encode_message(&ReviewCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .unwrap();
    let event_bytes =
        encode_message(&ReviewEventFrame(event.clone()), CodecLimits::PRODUCTION).unwrap();
    let state_bytes =
        encode_message(&ReviewStateFrame::from_state(&state), CodecLimits::PRODUCTION).unwrap();
    let corpus = [
        Fixture::new(
            "review-command.bin",
            command_bytes,
            COMMAND_BYTES,
            COMMAND_SHA256,
            COMMAND_TAG_OFFSET,
        ),
        Fixture::new("review-event.bin", event_bytes, EVENT_BYTES, EVENT_SHA256, EVENT_TAG_OFFSET),
        Fixture::new(
            "review-state.bin",
            state_bytes,
            STATE_BYTES,
            STATE_SHA256,
            STATE_PHASE_OFFSET,
        ),
    ];
    let root = fixture_root();
    let manifest = manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_REVIEW_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &manifest);
    }
    for fixture in &corpus {
        assert_eq!(fixture.bytes.len(), fixture.expected_len);
        assert_eq!(hex(sha256(&fixture.bytes).as_bytes()), fixture.expected_digest);
        assert_eq!(fs::read(root.join(fixture.name)).unwrap(), fixture.bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);

    assert_eq!(
        decode_message::<ReviewCommandFrame>(&corpus[0].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        command
    );
    assert_eq!(
        decode_message::<ReviewEventFrame>(&corpus[1].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        event
    );
    assert!(
        decode_message::<ReviewStateFrame>(&corpus[2].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .matches_state(&state)
    );
    reject_corruption::<ReviewCommandFrame>(&corpus[0]);
    reject_corruption::<ReviewEventFrame>(&corpus[1]);
    reject_corruption::<ReviewStateFrame>(&corpus[2]);
}

fn values() -> (ReviewCommand, crate::ReviewEvent, ReviewRunState) {
    let fixture = ContractFixture::new();
    let binding = fixture.binding();
    let command = ReviewCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        RunId::new(bytes(70)).unwrap(),
        0,
        None,
        digest(0),
        binding.revision(),
        ReviewCommandKind::StartRun { binding, limits: fixture.limits },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    (command, transition.event().clone(), transition.into_state())
}

struct ContractFixture {
    contract: AcceptanceContract,
    revision: RevisionTuple,
    limits: ReviewLimits,
    producer: ActorId,
}

impl ContractFixture {
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

    fn binding(&self) -> ReviewBinding {
        ReviewBinding::from_contract(
            &self.contract,
            self.revision,
            digest(90),
            digest(91),
            vec![self.producer],
            vec![digest(40)],
            self.limits,
        )
        .unwrap()
    }
}

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
    expected_len: usize,
    expected_digest: &'static str,
    tag_offset: usize,
}

impl Fixture {
    fn new(
        name: &'static str,
        bytes: Vec<u8>,
        expected_len: usize,
        expected_digest: &'static str,
        tag_offset: usize,
    ) -> Self {
        Self { name, bytes, expected_len, expected_digest, tag_offset }
    }
}

fn reject_corruption<T: peritus_codec::CanonicalDecode>(fixture: &Fixture) {
    let mut unknown = fixture.bytes.clone();
    unknown[fixture.tag_offset] = u8::MAX;
    assert!(decode_message::<T>(&unknown, CodecLimits::PRODUCTION).is_err());
    let mut trailing = fixture.bytes.clone();
    trailing.push(0);
    assert!(decode_message::<T>(&trailing, CodecLimits::PRODUCTION).is_err());
}

fn manifest(corpus: &[Fixture]) -> String {
    let mut output = String::from("# peritus review protocol compatibility corpus v1\n");
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

fn limits() -> ReviewLimits {
    ReviewLimits::new(
        16, 16, 16, 128, 16, 16, 16, 32, 16, 32, 256, 4_096, 4_096, 1_048_576, 4_194_304,
    )
    .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
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
