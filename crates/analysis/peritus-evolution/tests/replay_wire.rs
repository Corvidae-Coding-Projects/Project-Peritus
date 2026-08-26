//! Canonical F0 wire-family and replay tests.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CodecLimits, decode_message, encode_message, sha256,
};
use peritus_evolution::{
    CampaignCommandFrame, CampaignEventFrame, CampaignStateFrame, PointerCommandFrame,
    PointerEventFrame, PointerStateFrame, decide_campaign, decide_pointer,
};

use support::{HarnessFixture, campaign_genesis, digest, pointer_genesis};

#[test]
#[allow(clippy::too_many_lines, reason = "one test keeps all six F0 frame families paired")]
fn six_f0_frames_round_trip_and_reject_malformed_bytes() {
    assert_eq!(<CampaignCommandFrame as CanonicalEncode>::FAMILY, 88);
    assert_eq!(<CampaignEventFrame as CanonicalEncode>::FAMILY, 89);
    assert_eq!(<CampaignStateFrame as CanonicalEncode>::FAMILY, 90);
    assert_eq!(<PointerCommandFrame as CanonicalEncode>::FAMILY, 91);
    assert_eq!(<PointerEventFrame as CanonicalEncode>::FAMILY, 92);
    assert_eq!(<PointerStateFrame as CanonicalEncode>::FAMILY, 93);

    let fixture = HarnessFixture::new();
    let campaign = campaign_genesis(&fixture);
    let campaign_transition = decide_campaign(None, &campaign).expect("campaign transition");
    let pointer = pointer_genesis(&fixture, digest(120), digest(121));
    let pointer_transition = decide_pointer(None, &pointer).expect("pointer transition");

    let campaign_command = encode_message(
        &CampaignCommandFrame::from_command(&campaign).expect("campaign command frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("campaign command bytes");
    assert_eq!(
        decode_message::<CampaignCommandFrame>(&campaign_command, CodecLimits::PRODUCTION)
            .expect("decode campaign command")
            .into_command()
            .expect("check campaign command"),
        campaign,
    );
    let campaign_event = encode_message(
        &CampaignEventFrame::from_event(campaign_transition.event()).expect("campaign event frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("campaign event bytes");
    assert_eq!(
        decode_message::<CampaignEventFrame>(&campaign_event, CodecLimits::PRODUCTION)
            .expect("decode campaign event")
            .check(None)
            .expect("check campaign event"),
        *campaign_transition.event(),
    );
    let campaign_state = encode_message(
        &CampaignStateFrame::from_state(campaign_transition.state()).expect("campaign state frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("campaign state bytes");
    assert_eq!(
        decode_message::<CampaignStateFrame>(&campaign_state, CodecLimits::PRODUCTION)
            .expect("decode campaign state")
            .into_state(),
        *campaign_transition.state(),
    );

    let pointer_command = encode_message(
        &PointerCommandFrame::from_command(&pointer).expect("pointer command frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("pointer command bytes");
    assert_eq!(
        decode_message::<PointerCommandFrame>(&pointer_command, CodecLimits::PRODUCTION)
            .expect("decode pointer command")
            .into_command()
            .expect("check pointer command"),
        pointer,
    );
    let pointer_event = encode_message(
        &PointerEventFrame::from_event(pointer_transition.event()).expect("pointer event frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("pointer event bytes");
    assert_eq!(
        decode_message::<PointerEventFrame>(&pointer_event, CodecLimits::PRODUCTION)
            .expect("decode pointer event")
            .check(None)
            .expect("check pointer event"),
        *pointer_transition.event(),
    );
    let pointer_state = encode_message(
        &PointerStateFrame::from_state(pointer_transition.state()).expect("pointer state frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("pointer state bytes");
    assert_eq!(
        decode_message::<PointerStateFrame>(&pointer_state, CodecLimits::PRODUCTION)
            .expect("decode pointer state")
            .into_state(),
        *pointer_transition.state(),
    );

    let corpus = [
        ("campaign-command.bin", campaign_command.clone()),
        ("campaign-event.bin", campaign_event.clone()),
        ("campaign-state.bin", campaign_state.clone()),
        ("pointer-command.bin", pointer_command.clone()),
        ("pointer-event.bin", pointer_event.clone()),
        ("pointer-state.bin", pointer_state.clone()),
    ];
    let root = fixture_root();
    let sums = digest_manifest(&corpus);
    if std::env::var_os("PERITUS_UPDATE_EVOLUTION_FIXTURES").is_some() {
        write_corpus(&root, &corpus, &sums);
    }
    for (name, bytes) in &corpus {
        assert_eq!(fs::read(root.join(name)).expect("frozen F0 fixture"), *bytes);
    }
    assert_eq!(fs::read_to_string(root.join("SHA256SUMS")).expect("F0 digest inventory"), sums,);

    reject_envelope_corruption::<CampaignCommandFrame>(&campaign_command);
    reject_envelope_corruption::<CampaignEventFrame>(&campaign_event);
    reject_envelope_corruption::<CampaignStateFrame>(&campaign_state);
    reject_envelope_corruption::<PointerCommandFrame>(&pointer_command);
    reject_envelope_corruption::<PointerEventFrame>(&pointer_event);
    reject_envelope_corruption::<PointerStateFrame>(&pointer_state);
}

fn reject_envelope_corruption<T: CanonicalDecode>(bytes: &[u8]) {
    let mut wrong_family = bytes.to_vec();
    wrong_family[6..8].copy_from_slice(&999_u16.to_be_bytes());
    assert!(decode_message::<T>(&wrong_family, CodecLimits::PRODUCTION).is_err());

    let mut truncated = bytes.to_vec();
    truncated.pop();
    assert!(decode_message::<T>(&truncated, CodecLimits::PRODUCTION).is_err());

    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_message::<T>(&trailing, CodecLimits::PRODUCTION).is_err());
}

fn digest_manifest(corpus: &[(&str, Vec<u8>)]) -> String {
    let mut output = String::from("# peritus evolution protocol compatibility corpus v1\n");
    for (name, bytes) in corpus {
        output.push_str(&hex(sha256(bytes).as_bytes()));
        output.push_str("  ");
        output.push_str(name);
        output.push('\n');
    }
    output
}

fn write_corpus(root: &Path, corpus: &[(&str, Vec<u8>)], sums: &str) {
    fs::create_dir_all(root).expect("create F0 fixture directory");
    for (name, bytes) in corpus {
        fs::write(root.join(name), bytes).expect("write frozen F0 frame");
    }
    fs::write(root.join("SHA256SUMS"), sums).expect("write F0 digest inventory");
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("write digest hex");
        output
    })
}
