//! Terminal attach, stream, resize, detach, cancel, and exit integration tests.

mod support;

use peritus_app_protocol::{
    CorrelationId, RequestId, TerminalAttachmentId, TerminalBinding, TerminalCancellation,
    TerminalDetach, TerminalErrorKind, TerminalExit, TerminalExitDisposition, TerminalInput,
    TerminalOutput, TerminalPhase, TerminalResize, TerminalState, TerminalStream,
    TerminalTransitionDisposition,
};
use peritus_types::ProcessId;
use support::fixture_id;

fn binding_for(seed: u8) -> TerminalBinding {
    TerminalBinding::new(
        fixture_id(seed, TerminalAttachmentId::new),
        fixture_id(seed + 1, ProcessId::new),
        fixture_id(seed + 2, RequestId::new),
    )
}

#[test]
fn terminal_attach_stream_resize_detach_cancel_and_exit_ordering_is_exact() {
    let binding = binding_for(60);
    let mut terminal = TerminalState::new(binding, 8).expect("positive terminal chunk bound");
    assert_eq!(terminal.binding(), binding);
    terminal
        .accept_input(&TerminalInput::new(binding, b"ls\n".to_vec(), 8).unwrap())
        .expect("bounded attached input");
    terminal
        .resize(TerminalResize::new(binding, 120, 40, 240, 100).unwrap())
        .expect("positive bounded resize");
    assert_eq!(
        TerminalResize::new(binding, 0, 40, 240, 100)
            .expect_err("zero dimension is rejected")
            .kind(),
        TerminalErrorKind::InvalidInput,
    );

    let stdout = TerminalOutput::new(binding, 0, 0, TerminalStream::Stdout, b"ok".to_vec(), 8)
        .expect("bounded output");
    terminal.accept_output(&stdout).expect("first output is contiguous");
    assert_eq!(terminal.next_output_sequence(), 1);
    assert_eq!(terminal.next_output_offset(), 2);
    let wrong_sequence =
        TerminalOutput::new(binding, 2, 2, TerminalStream::Stderr, b"!".to_vec(), 8).unwrap();
    assert_eq!(
        terminal
            .accept_output(&wrong_sequence)
            .expect_err("skipped output sequence is rejected")
            .kind(),
        TerminalErrorKind::UnexpectedSequence,
    );
    let wrong_offset =
        TerminalOutput::new(binding, 1, 3, TerminalStream::Stderr, b"!".to_vec(), 8).unwrap();
    assert_eq!(
        terminal
            .accept_output(&wrong_offset)
            .expect_err("noncontiguous output offset is rejected")
            .kind(),
        TerminalErrorKind::UnexpectedOffset,
    );
    let stderr =
        TerminalOutput::new(binding, 1, 2, TerminalStream::Stderr, b"!".to_vec(), 8).unwrap();
    terminal.accept_output(&stderr).expect("second stream shares global ordering");

    let exit = TerminalExit::new(binding, 2, 3, TerminalExitDisposition::Code(0));
    terminal.exit(exit).expect("exit matches the exact final output fence");
    assert!(matches!(terminal.phase(), TerminalPhase::Exited(value) if value == exit));
    assert_eq!(
        terminal
            .accept_output(
                &TerminalOutput::new(binding, 2, 3, TerminalStream::Stdout, b"late".to_vec(), 8,)
                    .unwrap()
            )
            .expect_err("output after exit is impossible")
            .kind(),
        TerminalErrorKind::AlreadyTerminal,
    );
    assert_eq!(
        terminal.exit(exit).expect_err("a second exit is rejected").kind(),
        TerminalErrorKind::AlreadyTerminal,
    );

    let detached_binding = binding_for(70);
    let mut detached = TerminalState::new(detached_binding, 8).unwrap();
    let detach = TerminalDetach::new(detached_binding, fixture_id(73, CorrelationId::new));
    assert_eq!(
        detached.detach(detach).expect("detach applies"),
        TerminalTransitionDisposition::Applied,
    );
    assert_eq!(
        detached.detach(detach).expect("same detach is idempotent"),
        TerminalTransitionDisposition::Repeated,
    );
    assert_eq!(
        detached
            .accept_input(&TerminalInput::new(detached_binding, b"late".to_vec(), 8).unwrap())
            .expect_err("detached terminal rejects input")
            .kind(),
        TerminalErrorKind::AlreadyTerminal,
    );

    let cancelled_binding = binding_for(80);
    let mut cancelled = TerminalState::new(cancelled_binding, 8).unwrap();
    let cancellation =
        TerminalCancellation::new(cancelled_binding, fixture_id(83, CorrelationId::new));
    assert_eq!(
        cancelled.cancel(cancellation).expect("cancel applies"),
        TerminalTransitionDisposition::Applied,
    );
    assert_eq!(
        cancelled.cancel(cancellation).expect("same cancel is idempotent"),
        TerminalTransitionDisposition::Repeated,
    );

    let other_binding = binding_for(90);
    let attached = TerminalState::new(binding, 8).unwrap();
    assert_eq!(
        attached
            .accept_input(&TerminalInput::new(other_binding, b"x".to_vec(), 8).unwrap())
            .expect_err("input is attachment scoped")
            .kind(),
        TerminalErrorKind::BindingMismatch,
    );
}
