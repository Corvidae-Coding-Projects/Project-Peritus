//! Canonical families 85–87, malformed input, fixture, and replay coverage.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CodecLimits, decode_message, encode_message, sha256,
};
use peritus_eval::{
    EvaluationCommand, EvaluationCommandFrame, EvaluationCommandKind, EvaluationEventFrame,
    EvaluationStateFrame, apply_event, decide, replay,
};
use peritus_types::{CommandId, EventId};

use support::{artifact, bytes, campaign_id, digest, frozen_profile, revision};

fn genesis() -> EvaluationCommand {
    let profile = frozen_profile();
    EvaluationCommand::new(
        CommandId::new(bytes(100)).expect("command"),
        EventId::new(bytes(101)).expect("event"),
        campaign_id(),
        0,
        None,
        digest(0),
        profile.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: revision(),
            dataset_digest: profile.dataset().digest(),
            dataset_artifact: artifact(102),
            profile_artifact: artifact(103),
        },
    )
    .expect("genesis command")
}

#[test]
fn families_85_86_and_87_round_trip_through_semantic_checks() {
    assert_eq!(<EvaluationCommandFrame as CanonicalEncode>::FAMILY, 85);
    assert_eq!(<EvaluationEventFrame as CanonicalEncode>::FAMILY, 86);
    assert_eq!(<EvaluationStateFrame as CanonicalEncode>::FAMILY, 87);
    let command = genesis();
    let transition = decide(None, &command).expect("transition");
    let command_bytes = encode_message(
        &EvaluationCommandFrame::from_command(&command).expect("command frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("command bytes");
    assert_eq!(
        decode_message::<EvaluationCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .expect("decode command")
            .check()
            .expect("activate command"),
        command,
    );
    let event_bytes = encode_message(
        &EvaluationEventFrame::from_event(transition.event()).expect("event frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("event bytes");
    assert_eq!(
        decode_message::<EvaluationEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
            .expect("decode event")
            .check(None)
            .expect("activate event"),
        *transition.event(),
    );
    let state_bytes = encode_message(
        &EvaluationStateFrame::from_state(transition.state()),
        CodecLimits::PRODUCTION,
    )
    .expect("state bytes");
    assert_eq!(
        decode_message::<EvaluationStateFrame>(&state_bytes, CodecLimits::PRODUCTION)
            .expect("decode state")
            .into_state(),
        *transition.state(),
    );
    assert_eq!(apply_event(None, transition.event()).expect("apply"), *transition.state());
    assert_eq!(replay(&[transition.event().clone()]).expect("replay"), *transition.state());
}

#[test]
fn frozen_wire_corpus_is_byte_exact_and_malformed_frames_reject() {
    let command = genesis();
    let transition = decide(None, &command).expect("transition");
    let corpus = [
        (
            "evaluation-command.bin",
            encode_message(
                &EvaluationCommandFrame::from_command(&command).expect("frame"),
                CodecLimits::PRODUCTION,
            )
            .expect("command bytes"),
        ),
        (
            "evaluation-event.bin",
            encode_message(
                &EvaluationEventFrame::from_event(transition.event()).expect("frame"),
                CodecLimits::PRODUCTION,
            )
            .expect("event bytes"),
        ),
        (
            "evaluation-state.bin",
            encode_message(
                &EvaluationStateFrame::from_state(transition.state()),
                CodecLimits::PRODUCTION,
            )
            .expect("state bytes"),
        ),
    ];
    let root = fixture_root();
    let sums = digest_manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_EVALUATION_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &sums);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).expect("frozen fixture"), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).expect("sums"), sums);
    reject_wrong_family_or_trailing::<EvaluationCommandFrame>(&corpus[0].1);
    reject_wrong_family_or_trailing::<EvaluationEventFrame>(&corpus[1].1);
    reject_wrong_family_or_trailing::<EvaluationStateFrame>(&corpus[2].1);
    let mut corrupt = corpus[0].1.clone();
    *corrupt.last_mut().expect("bytes") ^= 1;
    assert!(
        decode_message::<EvaluationCommandFrame>(&corrupt, CodecLimits::PRODUCTION)
            .and_then(|frame| frame.check().map_err(|_| peritus_codec::CodecError::at(
                peritus_codec::CodecErrorKind::InvalidDomainValue,
                0,
            )))
            .is_err(),
    );
}

fn reject_wrong_family_or_trailing<T: CanonicalDecode>(bytes: &[u8]) {
    let mut wrong_family = bytes.to_vec();
    wrong_family[6..8].copy_from_slice(&999_u16.to_be_bytes());
    assert!(decode_message::<T>(&wrong_family, CodecLimits::PRODUCTION).is_err());
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_message::<T>(&trailing, CodecLimits::PRODUCTION).is_err());
}
fn digest_manifest(corpus: &[(&str, Vec<u8>)]) -> String {
    let mut output = String::from("# peritus evaluation protocol compatibility corpus v1\n");
    for (name, bytes) in corpus {
        output.push_str(&hex(sha256(bytes).as_bytes()));
        output.push_str("  ");
        output.push_str(name);
        output.push('\n');
    }
    output
}
fn write_corpus(root: &Path, corpus: &[(&str, Vec<u8>)], sums: &str) {
    fs::create_dir_all(root).expect("fixture directory");
    for (name, bytes) in corpus {
        fs::write(root.join(name), bytes).expect("fixture");
    }
    fs::write(root.join("SHA256SUMS"), sums).expect("digest inventory");
}
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("hex");
        output
    })
}
