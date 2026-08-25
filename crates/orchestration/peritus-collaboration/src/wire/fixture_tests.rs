//! Immutable family-73/74/75 compatibility corpus.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CanonicalDecode, CodecLimits, decode_message, encode_message, sha256};
use peritus_role::HarnessRole;
use peritus_scheduler::{SchedulerId, WorkId};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use super::{CollaborationCommandFrame, CollaborationEventFrame, CollaborationStateFrame};
use crate::{
    CollaborationBinding, CollaborationCommand, CollaborationCommandKind, CollaborationId,
    CollaborationLimits, CollaborationTaskId, Delegation, JoinPolicy, start,
};

const COMMAND_TAG_OFFSET: usize = 201;
const EVENT_TAG_OFFSET: usize = 233;

#[test]
fn canonical_corpus_is_exact_and_rejects_tag_and_trailing_corruption() {
    let (command, event, state) = values();
    let corpus = [
        Fixture::new(
            "collaboration-command.bin",
            encode_message(
                &CollaborationCommandFrame::from_command(&command),
                CodecLimits::PRODUCTION,
            )
            .unwrap(),
        ),
        Fixture::new(
            "collaboration-event.bin",
            encode_message(&CollaborationEventFrame(event.clone()), CodecLimits::PRODUCTION)
                .unwrap(),
        ),
        Fixture::new(
            "collaboration-state.bin",
            encode_message(&CollaborationStateFrame::from_state(&state), CodecLimits::PRODUCTION)
                .unwrap(),
        ),
    ];
    let root = fixture_root();
    let manifest = manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_COLLABORATION_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &manifest);
    }
    for fixture in &corpus {
        assert_eq!(fs::read(root.join(fixture.name)).unwrap(), fixture.bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);
    assert_eq!(
        decode_message::<CollaborationCommandFrame>(&corpus[0].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        command
    );
    assert_eq!(
        decode_message::<CollaborationEventFrame>(&corpus[1].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        event
    );
    assert!(
        decode_message::<CollaborationStateFrame>(&corpus[2].bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .matches_state(&state)
    );
    reject_tag::<CollaborationCommandFrame>(&corpus[0].bytes, COMMAND_TAG_OFFSET);
    reject_tag::<CollaborationEventFrame>(&corpus[1].bytes, EVENT_TAG_OFFSET);
    for fixture in &corpus {
        let mut trailing = fixture.bytes.clone();
        trailing.push(0);
        assert!(
            decode_message::<CollaborationStateFrame>(&trailing, CodecLimits::PRODUCTION).is_err()
        );
    }
}

fn values() -> (CollaborationCommand, crate::CollaborationEvent, crate::CollaborationState) {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(bytes(10)).unwrap(),
        HarnessId::new(bytes(11)).unwrap(),
        WorkspaceId::new(bytes(12)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(13)).unwrap(),
        ProviderProfileId::new(bytes(14)).unwrap(),
    );
    let run_id = RunId::new(bytes(15)).unwrap();
    let root_id = CollaborationTaskId::new(bytes(16)).unwrap();
    let owner = ActorId::new(bytes(17)).unwrap();
    let assignment = Delegation::root(
        root_id,
        owner,
        HarnessRole::Writer,
        WorkId::new(bytes(18)).unwrap(),
        digest(19),
        JoinPolicy::AllRequired,
    )
    .unwrap();
    let binding = CollaborationBinding::new(
        CollaborationId::new(bytes(20)).unwrap(),
        run_id,
        revision,
        SchedulerId::new(bytes(21)).unwrap(),
        CollaborationLimits::new(64, 8, 8, 128, 16, 65_536, 16, 1_048_576, 4_194_304).unwrap(),
        assignment,
    )
    .unwrap();
    let command = CollaborationCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        run_id,
        0,
        None,
        digest(0),
        revision,
        CollaborationCommandKind::Start { binding },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    (command, transition.event().clone(), transition.into_state())
}

fn reject_tag<T: CanonicalDecode>(bytes: &[u8], offset: usize) {
    let mut corrupt = bytes.to_vec();
    corrupt[offset] = u8::MAX;
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
    let mut output = String::from("# peritus collaboration protocol compatibility corpus v1\n");
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

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
