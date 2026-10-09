//! Native console observations through the actual C2 authority and supervisor path.
use super::*;
use peritus_process::OutputStream;

#[test]
fn windows_conpty_preserves_literal_arguments_input_resize_and_clean_completion() {
    let root = TestRoot::new();
    let ids = Ids::new(221);
    let mut options = control_options(IoMode::Pty(TerminalSize::new(24, 80, 0, 0).unwrap()));
    options.arguments = vec![
        "pty".into(),
        "space value".into(),
        "quote\"value".into(),
        "trailing\\".into(),
        String::new(),
        "Étage".into(),
    ];
    options.output_limit = 8192;
    let (owned, _) = launch(&root, &ids, plan(&root, &ids, options).unwrap());
    let control = owned.control();
    wait_output(&control, b"conpty-ready");
    control.resize(TerminalSize::new(31, 101, 0, 0).unwrap()).unwrap();
    control.write_stdin(b"pty-input".to_vec()).unwrap();
    let terminal = owned.wait().unwrap();
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    let bytes = terminal_bytes(&control);
    for expected in [
        "arg=11:space value",
        "arg=11:quote\"value",
        "arg=9:trailing\\",
        "arg=0:",
        "arg=6:Étage",
        "size=101x31",
        "pty-input",
    ] {
        assert!(
            bytes.windows(expected.len()).any(|part| part == expected.as_bytes()),
            "missing {expected}: {}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn windows_conpty_root_exit_reaps_its_live_descendant_before_terminal_publication() {
    let root = TestRoot::new();
    let ids = Ids::new(222);
    let mut options = control_options(IoMode::Pty(TerminalSize::new(24, 80, 0, 0).unwrap()));
    options.arguments = vec!["pipe-holder".into()];
    options.output_limit = 8192;
    options.process_count = 2;
    options.descendants = 1;
    let started = Instant::now();
    let (owned, _) = launch(&root, &ids, plan(&root, &ids, options).unwrap());
    let terminal = owned.wait().unwrap();
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    assert!(started.elapsed() < Duration::from_secs(10), "30-second descendant survived root exit");
}

fn terminal_bytes(control: &peritus_process::ProcessControl) -> Vec<u8> {
    crate::output(&control.read_events(ProcessCursor::after(0), 1024), OutputStream::Terminal)
}
fn wait_output(control: &peritus_process::ProcessControl, needle: &[u8]) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        if terminal_bytes(control).windows(needle.len()).any(|part| part == needle) {
            return;
        }
        assert!(control.terminal_result().is_none(), "console exited before readiness");
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("console readiness did not arrive");
}

#[test]
fn windows_conpty_cancellation_reaps_the_live_owned_tree() {
    let root = TestRoot::new();
    let ids = Ids::new(223);
    let mut options = control_options(IoMode::Pty(TerminalSize::new(24, 80, 0, 0).unwrap()));
    options.arguments = vec!["tree".into(), "2".into()];
    options.output_limit = 8192;
    options.process_count = 4;
    options.descendants = 3;
    let (owned, _) = launch(&root, &ids, plan(&root, &ids, options).unwrap());
    let control = owned.control();
    wait_output(&control, b"tree-ready");
    control.cancel(CancellationReason::User).unwrap();
    let terminal = owned.wait().unwrap();
    assert_eq!(terminal.disposition(), TerminalDisposition::Cancelled);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
}
