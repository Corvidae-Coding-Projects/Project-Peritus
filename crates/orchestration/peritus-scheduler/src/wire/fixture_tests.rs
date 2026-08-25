//! Immutable family-70/71/72 compatibility corpus.

#![allow(clippy::unwrap_used, reason = "fixed checked compatibility corpus")]

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use super::{SchedulerCommandFrame, SchedulerEventFrame, SchedulerStateFrame};
use crate::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandKind, SchedulerId, SchedulerLimits, start,
};

#[test]
fn canonical_binary_corpus_is_exact_and_rejects_wrong_family_or_trailing_bytes() {
    let limits = SchedulerLimits::new(8, 16, 4, 4, 4, 4, 3, 2, 2, 65_536, 262_144).unwrap();
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(bytes(10)).unwrap(),
        HarnessId::new(bytes(11)).unwrap(),
        WorkspaceId::new(bytes(12)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(13)).unwrap(),
        ProviderProfileId::new(bytes(14)).unwrap(),
    );
    let binding = SchedulerBinding::new(
        RunId::new(bytes(15)).unwrap(),
        SchedulerId::new(bytes(16)).unwrap(),
        revision,
        limits,
        ResourceVector::new(
            vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(4).unwrap())],
            limits.resource_dimensions(),
        )
        .unwrap(),
    )
    .unwrap();
    let command = SchedulerCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        binding.run_id(),
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        SchedulerCommandKind::StartScheduler { binding },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .unwrap();
    let event_bytes = encode_message(
        &SchedulerEventFrame::new(transition.event().clone()),
        CodecLimits::PRODUCTION,
    )
    .unwrap();
    let state_bytes = encode_message(
        &SchedulerStateFrame::from_state(transition.state()),
        CodecLimits::PRODUCTION,
    )
    .unwrap();
    let corpus = [
        ("scheduler-command.bin", command_bytes),
        ("scheduler-event.bin", event_bytes),
        ("scheduler-state.bin", state_bytes),
    ];
    let root = fixture_root();
    let manifest = manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_SCHEDULER_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &manifest);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).unwrap(), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);
    assert_eq!(
        decode_message::<SchedulerCommandFrame>(&corpus[0].1, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        command
    );
    assert_eq!(
        decode_message::<SchedulerEventFrame>(&corpus[1].1, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        *transition.event()
    );
    assert!(
        decode_message::<SchedulerStateFrame>(&corpus[2].1, CodecLimits::PRODUCTION)
            .unwrap()
            .matches_state(transition.state())
    );
    reject_wrong_family_or_trailing::<SchedulerCommandFrame>(&corpus[0].1);
    reject_wrong_family_or_trailing::<SchedulerEventFrame>(&corpus[1].1);
    reject_wrong_family_or_trailing::<SchedulerStateFrame>(&corpus[2].1);
}

fn reject_wrong_family_or_trailing<T: peritus_codec::CanonicalDecode>(bytes: &[u8]) {
    let mut wrong = bytes.to_vec();
    wrong[6..8].copy_from_slice(&999_u16.to_be_bytes());
    assert!(decode_message::<T>(&wrong, CodecLimits::PRODUCTION).is_err());
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_message::<T>(&trailing, CodecLimits::PRODUCTION).is_err());
}

fn manifest(corpus: &[(&str, Vec<u8>)]) -> String {
    let mut value = String::new();
    for (name, bytes) in corpus {
        value.push_str(&hex(sha256(bytes).as_bytes()));
        value.push_str("  ");
        value.push_str(name);
        value.push('\n');
    }
    value
}

fn write_corpus(root: &Path, corpus: &[(&str, Vec<u8>)], manifest: &str) {
    fs::create_dir_all(root).unwrap();
    for (name, bytes) in corpus {
        fs::write(root.join(name), bytes).unwrap();
    }
    fs::write(root.join("SHA256SUMS"), manifest).unwrap();
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").join("fixtures/protocol/scheduler-v1")
}

fn hex(bytes: &[u8]) -> String {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    value
}

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
