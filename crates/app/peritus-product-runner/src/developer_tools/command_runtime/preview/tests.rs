use std::{
    io::{BufRead as _, Write as _},
    thread,
    time::Instant,
};

use peritus_types::RunId;
use sha2::{Digest as _, Sha256};

use super::output::{OUTPUT_SEARCH_CHUNK_BYTES, StreamScan};
use super::*;
use crate::{PreviewOutputMatch, PreviewOutputMatchSource, PreviewOutputStream};

#[test]
#[ignore = "subprocess fixture; invoked by the owned-preview test"]
fn controlled_preview_fixture() {
    println!("READY state=0");
    std::io::stdout().flush().expect("flush readiness");
    let mut lines = std::io::BufReader::new(std::io::stdin()).lines();
    let movement = lines.next().expect("movement input").expect("movement bytes");
    println!("OBSERVED {movement} state=1");
    std::io::stdout().flush().expect("flush observed state");
    let _ = lines.next();
}

#[test]
#[ignore = "subprocess fixture; invoked by the owned-pipe-preview test"]
fn controlled_pipe_preview_fixture() {
    println!("READY state=0");
    std::io::stdout().flush().expect("flush readiness");
    loop {
        thread::park();
    }
}

#[test]
fn owned_preview_launches_accepts_input_and_stops_explicitly() {
    exercise_owned_preview(true);
}

#[test]
fn owned_pipe_preview_searches_only_its_available_live_streams() {
    exercise_owned_preview(false);
}

#[test]
fn streaming_match_finds_early_midstream_and_across_chunk_boundary() {
    for (offset, expected) in [(3_usize, 3_u64), (70_003, 70_003), (65_534, 65_534)] {
        let mut data = vec![b'x'; 140_000];
        data[offset..offset + 6].copy_from_slice(b"needle");
        let mut reads = 0;
        let found = scan_stream(
            b"needle",
            u64::try_from(data.len()).expect("stream length"),
            |start, count| {
                reads += 1;
                assert!(count <= OUTPUT_SEARCH_CHUNK_BYTES);
                let start = usize::try_from(start).expect("offset");
                Ok(data[start..start + count].to_vec())
            },
        )
        .expect("search");
        assert_eq!(found.match_start, Some(expected));
        assert_eq!(reads, 3, "source digest covers the full observed prefix");
        let expected_digest: [u8; 32] = Sha256::digest(&data).into();
        assert_eq!(found.digest, expected_digest);
    }
}

#[test]
fn streaming_large_miss_reads_bounded_chunks_without_lossy_utf8_aliases() {
    let data = vec![b'x'; 8 * 1024 * 1024];
    let mut reads = 0;
    let found = scan_stream(
        b"absent",
        u64::try_from(data.len()).expect("stream length"),
        |offset, count| {
            reads += 1;
            assert!(count <= OUTPUT_SEARCH_CHUNK_BYTES);
            let start = usize::try_from(offset).expect("offset");
            Ok(data[start..start + count].to_vec())
        },
    )
    .expect("large miss");
    assert_eq!(found.match_start, None);
    let expected_digest: [u8; 32] = Sha256::digest(&data).into();
    assert_eq!(found.digest, expected_digest);
    assert_eq!(reads, data.len().div_ceil(OUTPUT_SEARCH_CHUNK_BYTES));

    let invalid_utf8 = [0xff, 0xfe];
    assert_eq!(
        scan_stream(
            "�".as_bytes(),
            u64::try_from(invalid_utf8.len()).expect("stream length"),
            |offset, count| {
                let start = usize::try_from(offset).expect("offset");
                Ok(invalid_utf8[start..start + count].to_vec())
            },
        )
        .expect("raw-byte search"),
        StreamScan { match_start: None, digest: Sha256::digest(invalid_utf8).into() },
        "invalid source bytes must not match the replacement character created by lossy decoding"
    );
}

#[test]
fn output_match_evidence_round_trips_and_rejects_invalid_ranges() {
    let evidence = PreviewOutputMatch {
        stream: PreviewOutputStream::Stdout,
        start_byte: 12,
        end_byte: 17,
        observed_stream_bytes: 20,
        matched_bytes_digest: Sha256::digest(b"ready").into(),
        source: PreviewOutputMatchSource::LiveSpool {
            process_id: [91; 16],
            observed_prefix_digest: Sha256::digest(b"0123456789abcdefghij").into(),
        },
    };
    evidence.validate().expect("valid evidence");
    evidence
        .validate_for_needle(peritus_types::ProcessId::new([91; 16]).expect("process id"), "ready")
        .expect("process and needle binding");
    let encoded = serde_json::to_vec(&evidence).expect("encode");
    let decoded: PreviewOutputMatch = serde_json::from_slice(&encoded).expect("decode");
    assert_eq!(decoded, evidence);

    let mut malformed = evidence;
    malformed.end_byte = 21;
    assert!(malformed.validate().is_err());
    let mut encoded_value = serde_json::to_value(evidence).expect("encode value");
    encoded_value["source"]["live_spool"]["process_id"] =
        serde_json::json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let decoded: PreviewOutputMatch = serde_json::from_value(encoded_value).expect("decode");
    assert!(decoded.validate().is_err());
}

#[allow(
    clippy::too_many_lines,
    reason = "one subprocess regression keeps live, terminal, restart, and cleanup assertions together"
)]
fn exercise_owned_preview(interactive: bool) {
    let workspace = tempfile::tempdir().expect("workspace");
    let runtime_state = tempfile::tempdir().expect("runtime state");
    let run_id = RunId::new([91; 16]).expect("run");
    let processes = peritus_process::ProcessStore::open(
        runtime_state.path().join("processes"),
        workspace.path(),
    )
    .expect("process store");
    let runtime = CommandRuntime::open(
        runtime_state.path().join("router"),
        workspace.path(),
        run_id,
        processes,
    )
    .expect("runtime");
    let executable = std::env::current_exe().expect("current test executable");
    let command = PreviewCommand::new(
        executable.to_string_lossy().into_owned(),
        vec![
            "--ignored".to_owned(),
            "--exact".to_owned(),
            if interactive {
                "developer_tools::command_runtime::preview::tests::controlled_preview_fixture"
            } else {
                "developer_tools::command_runtime::preview::tests::controlled_pipe_preview_fixture"
            }
            .to_owned(),
            "--nocapture".to_owned(),
        ],
        workspace.path().to_path_buf(),
        Duration::from_secs(10),
        interactive,
        24,
        80,
        "owned-preview-normal-flow".to_owned(),
        Vec::new(),
    )
    .expect("profile");
    let launch = runtime.launch_preview(&command).expect("launch");
    assert_ne!(launch.process_id().as_bytes(), &[0; 16]);
    let began = Instant::now();
    let readiness_offset = loop {
        let observation = runtime.observe_preview(&launch).expect("live output");
        assert_eq!(observation.state(), PreviewProcessState::Running);
        if let Some(offset) = observation.stdout().find("READY state=0") {
            break u64::try_from(offset).expect("readiness offset");
        }
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "readiness must be visible before exit or input"
        );
        thread::sleep(Duration::from_millis(10));
    };
    let stream = if interactive { OutputStream::Terminal } else { OutputStream::Stdout };
    let live_range = runtime
        .preview_output_range(launch.process_id(), stream, readiness_offset, 9)
        .expect("live range");
    assert_eq!(live_range.bytes(), b"READY sta");
    assert!(live_range.total_bytes() >= 9);
    assert_eq!(live_range.digest(), None);
    assert!(
        runtime.preview_output_contains(launch.process_id(), "READY state=0").expect("live match")
    );
    let exact_match = runtime
        .preview_output_match(launch.process_id(), "READY state=0")
        .expect("live output evidence")
        .expect("live output match");
    assert_eq!(exact_match.stream().process_stream(), stream);
    assert_eq!(exact_match.start_byte(), readiness_offset);
    assert_eq!(exact_match.end_byte(), readiness_offset + 13);
    let expected_match_digest: [u8; 32] = Sha256::digest(b"READY state=0").into();
    assert_eq!(exact_match.matched_bytes_digest(), expected_match_digest);
    exact_match
        .validate_for_needle(launch.process_id(), "READY state=0")
        .expect("match is bound to the launched process and literal text");
    assert!(matches!(exact_match.source(), PreviewOutputMatchSource::LiveSpool { .. }));
    assert!(
        !runtime
            .preview_output_contains(launch.process_id(), "ABSENT preview marker")
            .expect("live miss")
    );
    if interactive {
        #[cfg(windows)]
        let input = b"MOVE_RIGHT\r\n".as_slice();
        #[cfg(not(windows))]
        let input = b"MOVE_RIGHT\n".as_slice();
        let observation = runtime.interact_preview(&launch, input.to_vec()).expect("interaction");
        assert_eq!(observation.state(), PreviewProcessState::Running);
        // Observe the child's response even when interact_preview consumed the acknowledgement.
        wait_for_output(&runtime, &launch, "OBSERVED MOVE_RIGHT state=1");
    }
    let terminal = runtime.stop_preview(&launch).expect("explicit stop");
    let terminal = wait_for_terminal(&runtime, &launch, terminal);
    assert_eq!(terminal.state(), PreviewProcessState::Cancelled);
    assert!(terminal.stdout().contains("READY state=0"));
    if interactive {
        assert!(terminal.stdout().contains("OBSERVED MOVE_RIGHT state=1"));
    }
    let retained_range = runtime
        .preview_output_range(launch.process_id(), stream, readiness_offset, 9)
        .expect("finalized range");
    assert_eq!(retained_range.bytes(), b"READY sta");
    assert_eq!(retained_range.digest().map(|value| value.len()), Some(32));
    let finalized_match = runtime
        .preview_output_match(launch.process_id(), "READY state=0")
        .expect("finalized output evidence")
        .expect("finalized output match");
    assert_eq!(finalized_match.start_byte(), readiness_offset);
    assert_eq!(finalized_match.end_byte(), readiness_offset + 13);
    assert!(matches!(finalized_match.source(), PreviewOutputMatchSource::FinalizedArtifact { .. }));
    assert_eq!(finalized_match.artifact_digest(), Some(finalized_match.observed_source_digest()));
    drop(runtime);

    let recovered_processes = peritus_process::ProcessStore::open(
        runtime_state.path().join("processes"),
        workspace.path(),
    )
    .expect("reopen process store");
    let recovered = CommandRuntime::open(
        runtime_state.path().join("router"),
        workspace.path(),
        run_id,
        recovered_processes,
    )
    .expect("reopen runtime");
    let recovered_match = recovered
        .preview_output_match(launch.process_id(), "READY state=0")
        .expect("search recovered artifact")
        .expect("recovered match");
    assert_eq!(recovered_match, finalized_match);
    recovered
        .verify_preview_output_match(exact_match, "READY state=0")
        .expect("live prefix evidence survives restart through the finalized artifact");
    recovered
        .verify_preview_output_match(recovered_match, "READY state=0")
        .expect("finalized artifact evidence survives restart");
    let mut forged_live_match = exact_match;
    forged_live_match.source = PreviewOutputMatchSource::LiveSpool {
        process_id: *launch.process_id().as_bytes(),
        observed_prefix_digest: [0; 32],
    };
    assert!(recovered.verify_preview_output_match(forged_live_match, "READY state=0").is_err());
}

fn wait_for_output(runtime: &CommandRuntime, launch: &PreviewLaunch, expected: &str) {
    let began = Instant::now();
    loop {
        let observed = runtime.observe_preview(launch).expect("observe");
        if observed.stdout().contains(expected) {
            return;
        }
        assert!(began.elapsed() < Duration::from_secs(5), "missing {expected}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_terminal(
    runtime: &CommandRuntime,
    launch: &PreviewLaunch,
    mut observed: PreviewObservation,
) -> PreviewObservation {
    let began = Instant::now();
    while observed.state() == PreviewProcessState::Running {
        assert!(began.elapsed() < Duration::from_secs(5), "preview did not stop");
        thread::sleep(Duration::from_millis(10));
        observed = runtime.observe_preview(launch).expect("observe terminal");
    }
    observed
}
