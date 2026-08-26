//! Canonical wire, replay, projection, and corruption-rejection tests.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]

mod fixtures_support;

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CanonicalEncode, CodecLimits, decode_message, encode_message, sha256};
use peritus_harness::{
    HarnessCommand, HarnessCommandFrame, HarnessCommandKind, HarnessEventFrame, HarnessProjection,
    HarnessStateFrame, decide,
};
use peritus_types::Sha256Digest;

// Header + three identities + sequence + predecessor option + two digests.
const GENESIS_COMMAND_KIND_OFFSET: usize = 16 + 16 + 16 + 16 + 8 + 1 + 32 + 32;

fn genesis_transition() -> (HarnessCommand, peritus_harness::HarnessTransition) {
    let (revision, _) = fixtures_support::genesis_fixture();
    let command = HarnessCommand::new(
        fixtures_support::command_id(1),
        fixtures_support::event_id(2),
        revision.harness_id(),
        0,
        None,
        Sha256Digest::new([0; 32]),
        HarnessCommandKind::RegisterGenesis { revision },
    )
    .unwrap();
    let transition = decide(None, &command).unwrap();
    (command, transition)
}

#[test]
fn families_79_80_81_round_trip_only_through_checked_activation() {
    assert_eq!(HarnessCommandFrame::FAMILY, 79);
    assert_eq!(HarnessEventFrame::FAMILY, 80);
    assert_eq!(HarnessStateFrame::FAMILY, 81);
    let (command, transition) = genesis_transition();

    let command_bytes = encode_message(
        &HarnessCommandFrame::from_command(&command).unwrap(),
        CodecLimits::PRODUCTION,
    )
    .unwrap();
    let checked_command =
        decode_message::<HarnessCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .check(None)
            .unwrap();
    assert_eq!(checked_command, command);

    let event_bytes = encode_message(
        &HarnessEventFrame::from_event(transition.event()).unwrap(),
        CodecLimits::PRODUCTION,
    )
    .unwrap();
    let checked_event = decode_message::<HarnessEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
        .unwrap()
        .check(None)
        .unwrap();
    assert_eq!(&checked_event, transition.event());

    let state_bytes =
        encode_message(&HarnessStateFrame::from_state(transition.state()), CodecLimits::PRODUCTION)
            .unwrap();
    let checked_state = decode_message::<HarnessStateFrame>(&state_bytes, CodecLimits::PRODUCTION)
        .unwrap()
        .into_state();
    assert_eq!(&checked_state, transition.state());
}

#[test]
fn canonical_corpus_is_exact_and_rejects_corruption() {
    let (command, transition) = genesis_transition();
    let corpus = [
        (
            "harness-command.bin",
            encode_message(
                &HarnessCommandFrame::from_command(&command).unwrap(),
                CodecLimits::PRODUCTION,
            )
            .unwrap(),
        ),
        (
            "harness-event.bin",
            encode_message(
                &HarnessEventFrame::from_event(transition.event()).unwrap(),
                CodecLimits::PRODUCTION,
            )
            .unwrap(),
        ),
        (
            "harness-state.bin",
            encode_message(
                &HarnessStateFrame::from_state(transition.state()),
                CodecLimits::PRODUCTION,
            )
            .unwrap(),
        ),
    ];
    let root = fixture_root();
    let manifest = manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_HARNESS_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &manifest);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).unwrap(), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);

    assert_eq!(
        decode_message::<HarnessCommandFrame>(&corpus[0].1, CodecLimits::PRODUCTION)
            .unwrap()
            .check(None)
            .unwrap(),
        command,
    );
    assert_eq!(
        decode_message::<HarnessEventFrame>(&corpus[1].1, CodecLimits::PRODUCTION)
            .unwrap()
            .check(None)
            .unwrap(),
        *transition.event(),
    );
    assert_eq!(
        decode_message::<HarnessStateFrame>(&corpus[2].1, CodecLimits::PRODUCTION)
            .unwrap()
            .into_state(),
        *transition.state(),
    );

    reject_wrong_family_or_trailing::<HarnessCommandFrame>(&corpus[0].1);
    reject_wrong_family_or_trailing::<HarnessEventFrame>(&corpus[1].1);
    reject_wrong_family_or_trailing::<HarnessStateFrame>(&corpus[2].1);

    let mut unknown_kind = corpus[0].1.clone();
    unknown_kind[GENESIS_COMMAND_KIND_OFFSET] = u8::MAX;
    assert!(decode_message::<HarnessCommandFrame>(&unknown_kind, CodecLimits::PRODUCTION).is_err());
}

#[test]
fn replay_projection_matches_live_state_and_rejects_duplicate_genesis() {
    let (_, transition) = genesis_transition();
    let events = vec![transition.event().clone()];
    let replayed = peritus_harness::replay::replay(&events).unwrap();
    assert_eq!(&replayed, transition.state());
    let projection = HarnessProjection::rebuild(&events).unwrap();
    assert_eq!(projection.state_digest(), transition.state().state_digest());
    assert_eq!(projection.revisions().len(), 1);
    assert!(peritus_harness::replay::replay(&[events[0].clone(), events[0].clone()]).is_err());
}

#[test]
fn complete_frame_trailing_bytes_are_rejected() {
    let (command, _) = genesis_transition();
    let mut bytes = encode_message(
        &HarnessCommandFrame::from_command(&command).unwrap(),
        CodecLimits::PRODUCTION,
    )
    .unwrap();
    bytes.push(0);
    assert!(decode_message::<HarnessCommandFrame>(&bytes, CodecLimits::PRODUCTION).is_err());
}

fn reject_wrong_family_or_trailing<T: peritus_codec::CanonicalDecode>(bytes: &[u8]) {
    let mut wrong_family = bytes.to_vec();
    wrong_family[6..8].copy_from_slice(&999_u16.to_be_bytes());
    assert!(decode_message::<T>(&wrong_family, CodecLimits::PRODUCTION).is_err());

    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_message::<T>(&trailing, CodecLimits::PRODUCTION).is_err());
}

fn manifest(corpus: &[(&str, Vec<u8>)]) -> String {
    let mut output = String::from("# peritus harness protocol compatibility corpus v1\n");
    for (name, bytes) in corpus {
        output.push_str(&hex(sha256(bytes).as_bytes()));
        output.push_str("  ");
        output.push_str(name);
        output.push('\n');
    }
    output
}

fn write_corpus(root: &Path, corpus: &[(&str, Vec<u8>)], manifest: &str) {
    fs::create_dir_all(root).unwrap();
    for (name, bytes) in corpus {
        fs::write(root.join(name), bytes).unwrap();
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
