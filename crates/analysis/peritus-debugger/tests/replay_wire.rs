//! Canonical family 82/83/84 wire and replay coverage.

use std::{
    fs,
    path::{Path, PathBuf},
};

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CodecLimits, decode_message, encode_message, sha256,
};
use peritus_debugger::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandFrame, DebuggerCommandKind, DebuggerEventFrame,
    DebuggerJobId, DebuggerPhase, DebuggerStateFrame, SelectionManifestId, SelectionRecord,
    apply_event, decide, replay,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).expect("acceptance identity"),
        HarnessId::new(bytes(2)).expect("harness identity"),
        WorkspaceId::new(bytes(3)).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(4)).expect("policy identity"),
        ProviderProfileId::new(bytes(5)).expect("provider identity"),
    )
}

fn genesis() -> DebuggerCommand {
    DebuggerCommand::new(
        CommandId::new(bytes(10)).expect("command identity"),
        EventId::new(bytes(11)).expect("event identity"),
        DebuggerJobId::new(bytes(12)).expect("job identity"),
        0,
        None,
        digest(0),
        digest(13),
        DebuggerCommandKind::CreateJob {
            revision: revision(),
            query_digest: digest(13),
            limits_digest: digest(14),
            model_plan_digest: None,
        },
    )
    .expect("valid genesis command")
}

fn next(
    state: &peritus_debugger::DebuggerState,
    seed: u8,
    kind: DebuggerCommandKind,
) -> DebuggerCommand {
    DebuggerCommand::new(
        CommandId::new(bytes(seed)).expect("command identity"),
        EventId::new(bytes(seed.wrapping_add(1))).expect("event identity"),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        kind,
    )
    .expect("valid fenced command")
}

#[test]
fn families_82_83_and_84_round_trip_through_semantic_checks() {
    assert_eq!(<DebuggerCommandFrame as CanonicalEncode>::FAMILY, 82);
    assert_eq!(<DebuggerEventFrame as CanonicalEncode>::FAMILY, 83);
    assert_eq!(<DebuggerStateFrame as CanonicalEncode>::FAMILY, 84);
    let command = genesis();
    let command_bytes = encode_message(
        &DebuggerCommandFrame::from_command(&command).expect("command frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("encode family 82");
    let decoded_command =
        decode_message::<DebuggerCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .expect("decode family 82")
            .check()
            .expect("check family 82");
    assert_eq!(decoded_command, command);

    let transition = decide(None, &command).expect("genesis transition");
    let event_bytes = encode_message(
        &DebuggerEventFrame::from_event(transition.event()).expect("event frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("encode family 83");
    let decoded_event = decode_message::<DebuggerEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
        .expect("decode family 83")
        .check(None)
        .expect("check family 83");
    assert_eq!(&decoded_event, transition.event());

    let state_bytes = encode_message(
        &DebuggerStateFrame::from_state(transition.state()),
        CodecLimits::PRODUCTION,
    )
    .expect("encode family 84");
    let decoded_state = decode_message::<DebuggerStateFrame>(&state_bytes, CodecLimits::PRODUCTION)
        .expect("decode family 84")
        .into_state();
    assert_eq!(&decoded_state, transition.state());
}

#[test]
fn frozen_family_corpus_matches_exact_canonical_bytes() {
    let command = genesis();
    let transition = decide(None, &command).expect("genesis transition");
    let corpus = [
        (
            "debugger-command.bin",
            encode_message(
                &DebuggerCommandFrame::from_command(&command).expect("command frame"),
                CodecLimits::PRODUCTION,
            )
            .expect("encode command fixture"),
        ),
        (
            "debugger-event.bin",
            encode_message(
                &DebuggerEventFrame::from_event(transition.event()).expect("event frame"),
                CodecLimits::PRODUCTION,
            )
            .expect("encode event fixture"),
        ),
        (
            "debugger-state.bin",
            encode_message(
                &DebuggerStateFrame::from_state(transition.state()),
                CodecLimits::PRODUCTION,
            )
            .expect("encode state fixture"),
        ),
    ];
    let root = fixture_root();
    let sums = digest_manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_DEBUGGER_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &sums);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).expect("read frozen frame"), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).expect("read digest inventory"), sums,);
    assert_eq!(
        decode_message::<DebuggerCommandFrame>(&corpus[0].1, CodecLimits::PRODUCTION)
            .expect("decode command fixture")
            .check()
            .expect("activate command fixture"),
        command,
    );
    assert_eq!(
        decode_message::<DebuggerEventFrame>(&corpus[1].1, CodecLimits::PRODUCTION)
            .expect("decode event fixture")
            .check(None)
            .expect("activate event fixture"),
        *transition.event(),
    );
    assert_eq!(
        decode_message::<DebuggerStateFrame>(&corpus[2].1, CodecLimits::PRODUCTION)
            .expect("decode state fixture")
            .into_state(),
        *transition.state(),
    );
    reject_wrong_family_or_trailing::<DebuggerCommandFrame>(&corpus[0].1);
    reject_wrong_family_or_trailing::<DebuggerEventFrame>(&corpus[1].1);
    reject_wrong_family_or_trailing::<DebuggerStateFrame>(&corpus[2].1);
}

#[test]
fn event_replay_reconstructs_the_exact_successor_chain() {
    let created = decide(None, &genesis()).expect("create job");
    let selection = SelectionRecord::new(
        SelectionManifestId::new(bytes(20)).expect("manifest identity"),
        digest(21),
        1,
        4,
    )
    .expect("selection record");
    let selected_command =
        next(created.state(), 22, DebuggerCommandKind::RecordSelection { selection });
    let selected = decide(Some(created.state()), &selected_command).expect("record selection");
    let analysis_command = next(
        selected.state(),
        24,
        DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(25),
            counts: AnalysisCounts::new(3, 2, 1),
        },
    );
    let analyzed = decide(Some(selected.state()), &analysis_command).expect("record analysis");
    let events = vec![created.event().clone(), selected.event().clone(), analyzed.event().clone()];

    assert_eq!(
        apply_event(Some(selected.state()), analyzed.event()).expect("apply last event"),
        analyzed.state().clone(),
    );
    let rebuilt = replay(&events).expect("replay contiguous history");
    assert_eq!(rebuilt, analyzed.state().clone());
    assert_eq!(rebuilt.phase(), DebuggerPhase::DeterministicComplete);
    assert_eq!(rebuilt.sequence(), 3);
}

#[test]
fn wire_rejects_corruption_instead_of_partially_activating_it() {
    let command = genesis();
    let mut bytes = encode_message(
        &DebuggerCommandFrame::from_command(&command).expect("command frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("encode command");
    let last = bytes.last_mut().expect("nonempty frame");
    *last ^= 0x01;
    assert!(
        decode_message::<DebuggerCommandFrame>(&bytes, CodecLimits::PRODUCTION).is_err(),
        "canonical frame corruption must fail before semantic activation",
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
    let mut output = String::from("# peritus debugger protocol compatibility corpus v1\n");
    for (name, bytes) in corpus {
        output.push_str(&hex(sha256(bytes).as_bytes()));
        output.push_str("  ");
        output.push_str(name);
        output.push('\n');
    }
    output
}

fn write_corpus(root: &Path, corpus: &[(&str, Vec<u8>)], sums: &str) {
    fs::create_dir_all(root).expect("create fixture directory");
    for (name, bytes) in corpus {
        fs::write(root.join(name), bytes).expect("write frozen frame");
    }
    fs::write(root.join("SHA256SUMS"), sums).expect("write digest inventory");
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wire-v1")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("write to string");
        output
    })
}
