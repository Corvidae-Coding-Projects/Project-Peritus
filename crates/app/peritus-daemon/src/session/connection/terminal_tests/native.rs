//! Real PTY backpressure isolates one attachment while its sibling keeps delivering.

use super::*;
use peritus_process::ProcessStore;
use peritus_product_runner::{CommandRuntime, PreviewCommand};
use peritus_types::RunId;
use std::io::Write as _;
use std::time::{Duration, Instant};

#[test]
#[ignore = "owned PTY subprocess fixture invoked by the backpressure regression"]
fn burst_fixture() {
    println!("READY");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "burst");
    println!("{}BURST DONE", "x".repeat(4096));
    std::io::stdout().flush().unwrap();
    line.clear();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "finish");
}

#[tokio::test]
async fn backpressured_attachment_preserves_live_process_and_healthy_sibling() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let processes = ProcessStore::open(state.path().join("processes"), workspace.path()).unwrap();
    let runtime = CommandRuntime::open(
        state.path().join("router"),
        workspace.path(),
        RunId::new([77; 16]).unwrap(),
        processes,
    )
    .unwrap();
    let command = PreviewCommand::new(
        std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        [
            "--ignored",
            "--exact",
            "session::connection::terminal_tests::native::burst_fixture",
            "--nocapture",
        ]
        .map(str::to_owned)
        .to_vec(),
        workspace.path().to_path_buf(),
        Duration::from_secs(30),
        true,
        24,
        80,
        "attachment-isolation".to_owned(),
        Vec::new(),
    )
    .unwrap();
    let launch = runtime.launch_preview(&command).unwrap();
    let lease = runtime.preview_terminal(&launch).unwrap();
    let control = lease.control();
    let wait_for = |expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let observation = runtime.observe_preview(&launch).unwrap();
            if observation.stdout().contains(expected) {
                break;
            }
            assert!(Instant::now() < deadline, "missing fixture output: {expected}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    wait_for("READY");
    let actor = ActorId::new([77; 16]).unwrap();
    let session = SessionId::new([77; 16]).unwrap();
    let context = ProtocolContext::new(
        ProtocolId::new([77; 16]).unwrap(),
        ProtocolVersion::new(1, 0).unwrap(),
        session,
    );
    let binding = |value| {
        TerminalBinding::new(
            TerminalAttachmentId::new([value; 16]).unwrap(),
            launch.process_id(),
            RequestId::new([value; 16]).unwrap(),
        )
    };
    let slow = binding(77);
    let healthy = binding(78);
    let registry =
        TerminalRegistry::new(crate::terminal::TerminalRegistryLimits::PRODUCTION).unwrap();
    registry.register_preview(actor, session, lease).unwrap();
    registry.attach(actor, session, slow, 1).unwrap();
    registry.attach(actor, session, healthy, 8192).unwrap();
    control.write_stdin(b"burst\n".to_vec()).unwrap();
    wait_for("BURST DONE");
    let (stream, peer) = tokio::io::duplex(1024 * 1024);
    let mut frames = crate::AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let mut peer = crate::AppFrameStream::new(peer, AppProtocolLimits::PRODUCTION);
    let mut bindings = vec![slow, healthy];
    for _ in 0..12 {
        pump_terminals(&mut frames, &registry, &mut bindings, actor, session, context, true)
            .await
            .unwrap();
        if !bindings.contains(&slow) {
            break;
        }
    }
    assert_eq!(bindings, vec![healthy]);
    assert_eq!(registry.counts(), (1, 1));
    assert!(control.terminal_result().is_none(), "output failure must not kill the process");
    let mut output = Vec::new();
    loop {
        let AppMessage::Event(event) = peer.read().await.unwrap() else { panic!("terminal event") };
        match event.payload() {
            AppEventPayload::TerminalOutput(chunk) if chunk.binding() == healthy => {
                output.extend_from_slice(chunk.bytes());
            }
            AppEventPayload::TerminalUnavailable(failed) => {
                assert_eq!(*failed, slow);
                break;
            }
            AppEventPayload::TerminalExited(_) => panic!("live process cannot be reported exited"),
            _ => {}
        }
    }
    assert!(String::from_utf8_lossy(&output).contains("BURST DONE"));
    control.write_stdin(b"finish\n".to_vec()).unwrap();
    registry.shutdown().unwrap();
}
