//! Immutable family-70/71/72 compatibility corpus.

#![allow(clippy::unwrap_used, reason = "fixed checked compatibility corpus")]

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CodecError, CodecLimits, sha256};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use super::{
    decode_scheduler_command, decode_scheduler_event, decode_scheduler_state,
    encode_scheduler_command, encode_scheduler_event, encode_scheduler_state,
};
use crate::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandKind, SchedulerId, SchedulerLimits, SchedulerSemantics,
    start,
};

#[test]
fn canonical_binary_corpus_is_exact_and_rejects_wrong_family_or_trailing_bytes() {
    check_corpus(SchedulerSemantics::LegacyQueueV1);
    check_corpus(SchedulerSemantics::StrictRecoveryQueueV2);
}

fn check_corpus(semantics: SchedulerSemantics) {
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
    let binding = SchedulerBinding::from_wire(
        semantics,
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
    let command = SchedulerCommand::from_wire(
        semantics,
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        binding.run_id(),
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        SchedulerCommandKind::StartScheduler { binding },
    );
    let transition = start(&command).unwrap();
    let command_bytes = encode_scheduler_command(&command, CodecLimits::PRODUCTION).unwrap();
    let event_bytes = encode_scheduler_event(transition.event(), CodecLimits::PRODUCTION).unwrap();
    let state_bytes = encode_scheduler_state(transition.state(), CodecLimits::PRODUCTION).unwrap();
    let corpus = [
        ("scheduler-command.bin", command_bytes),
        ("scheduler-event.bin", event_bytes),
        ("scheduler-state.bin", state_bytes),
    ];
    let root = fixture_root(semantics);
    let manifest = manifest(&corpus);
    if semantics == SchedulerSemantics::StrictRecoveryQueueV2
        && std::env::var_os("PERITUS_UPDATE_SCHEDULER_V2_FIXTURES").is_some()
    {
        write_corpus(&root, &corpus, &manifest);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).unwrap(), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).unwrap(), manifest);
    assert_eq!(decode_scheduler_command(&corpus[0].1, CodecLimits::PRODUCTION).unwrap(), command);
    assert_eq!(
        decode_scheduler_event(&corpus[1].1, CodecLimits::PRODUCTION).unwrap(),
        *transition.event()
    );
    assert_eq!(
        decode_scheduler_state(&corpus[2].1, CodecLimits::PRODUCTION).unwrap(),
        *transition.state()
    );
    reject_wrong_family_or_trailing(&corpus[0].1, |bytes| {
        decode_scheduler_command(bytes, CodecLimits::PRODUCTION)
    });
    reject_wrong_family_or_trailing(&corpus[1].1, |bytes| {
        decode_scheduler_event(bytes, CodecLimits::PRODUCTION)
    });
    reject_wrong_family_or_trailing(&corpus[2].1, |bytes| {
        decode_scheduler_state(bytes, CodecLimits::PRODUCTION)
    });
}

fn reject_wrong_family_or_trailing<T>(
    bytes: &[u8],
    decode: impl Fn(&[u8]) -> Result<T, CodecError>,
) {
    let mut wrong = bytes.to_vec();
    wrong[6..8].copy_from_slice(&999_u16.to_be_bytes());
    assert!(decode(&wrong).is_err());
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
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

fn fixture_root(semantics: SchedulerSemantics) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(format!("fixtures/protocol/scheduler-v{}", semantics.schema_version()))
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
