//! Output observation is available as soon as an authorized owner is returned.

use super::*;
use peritus_process::OutputStream;

#[test]
fn pipe_spools_are_readable_immediately_after_launch() {
    check_startup(
        IoMode::Pipes,
        &[OutputStream::Stdout, OutputStream::Stderr],
        OutputStream::Terminal,
    );
}

#[test]
fn terminal_spool_is_readable_immediately_after_launch() {
    check_startup(
        IoMode::Pty(TerminalSize::new(24, 80, 0, 0).expect("terminal size")),
        &[OutputStream::Terminal],
        OutputStream::Stdout,
    );
}

fn check_startup(io: IoMode, streams: &[OutputStream], absent: OutputStream) {
    let root = TestRoot::new();
    let ids = Ids::new(205);
    let mut options = control_options(io);
    if matches!(io, IoMode::Pty(_)) {
        // Match the native PTY regressions: terminal setup can emit control bytes even
        // for this silent fixture, and this scenario checks readability rather than limits.
        options.output_limit = 8192;
    }
    let execution = plan(&root, &ids, options).expect("quiet process plan");
    let (owned, store) = launch(&root, &ids, execution);
    let control = owned.control();
    // Read immediately, without waiting for native launch or a first output event.
    for &stream in streams {
        let initial = control.spooled_stream_range(stream, 0, 64).expect("initial range");
        let full = control.full_spooled_stream_output(stream).expect("initial stream");
        let exact = store
            .spooled_stream_range_exact(ids.run, ids.action, ids.process, stream, 0, 64)
            .expect("initial exact-owner range");
        if stream == OutputStream::Terminal {
            // Appends may occur between observations; earlier bytes must remain unchanged.
            let full_length = u64::try_from(full.len()).expect("spool length");
            assert_eq!(initial.1.len(), usize::try_from(initial.0.min(64)).unwrap());
            assert!(initial.0 <= full_length, "terminal spool shrank before the full read");
            assert!(full.starts_with(&initial.1), "initial terminal prefix changed");
            assert!(full_length <= exact.0, "terminal spool shrank before the exact-owner read");
            assert_eq!(exact.1.len(), usize::try_from(exact.0.min(64)).unwrap());
            assert!(
                exact.1.starts_with(&full[..full.len().min(64)]),
                "exact-owner terminal prefix differs from the full read"
            );
        } else {
            assert_eq!(initial, (0, vec![]));
            assert!(full.is_empty());
            assert_eq!(exact, (0, vec![]));
        }
    }
    assert!(control.spooled_stream_range(absent, 0, 64).is_err());
    control.cancel(CancellationReason::User).expect("cancel quiet process");
    let terminal = owned.wait().expect("joined process");
    assert_eq!(terminal.disposition(), TerminalDisposition::Cancelled);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
}
