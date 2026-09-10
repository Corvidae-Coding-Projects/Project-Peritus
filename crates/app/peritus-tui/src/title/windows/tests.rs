//! Native title ownership, nesting, foreground handoff, and error-exit regressions.

use std::{io, os::windows::process::CommandExt as _, process::Command};

use super::SavedTitle;
use crate::TerminalTitle;

const CHILD: &str = "PERITUS_TITLE_OWNER_TEST_CHILD";

#[test]
fn title_is_restored_after_foreground_handoff_and_error() {
    if std::env::var_os(CHILD).is_none() {
        run_child(
            "title::windows::tests::title_is_restored_after_foreground_handoff_and_error",
            0x0000_0010, // CREATE_NEW_CONSOLE
        );
        return;
    }
    for original in ["PowerShell — 工作 🦀", ""] {
        let original = SavedTitle(original.encode_utf16().chain(Some(0)).collect());
        original.restore().expect("set original title");
        let result: io::Result<()> = (|| {
            let owner = TerminalTitle::acquire()?;
            assert_eq!(current(), SavedTitle::peritus());
            {
                let _login = TerminalTitle::acquire()?;
                SavedTitle("claude".encode_utf16().chain(Some(0)).collect()).restore()?;
            }
            assert_eq!(current(), SavedTitle::peritus(), "login must restore its caller");
            SavedTitle("foreground command".encode_utf16().chain(Some(0)).collect()).restore()?;
            owner.activate()?;
            assert_eq!(current(), SavedTitle::peritus(), "resume must reclaim the title");
            Err(io::Error::other("simulated launch failure"))
        })();
        assert!(result.is_err());
        assert_eq!(current(), original, "an error exit must restore the exact shell title");
    }
}

#[test]
fn process_without_console_does_not_acquire_a_title() {
    if std::env::var_os(CHILD).is_none() {
        run_child(
            "title::windows::tests::process_without_console_does_not_acquire_a_title",
            0x0800_0000, // CREATE_NO_WINDOW
        );
        return;
    }
    assert_eq!(SavedTitle::capture().expect("observe detached console"), None);
    let owner = TerminalTitle::acquire().expect("no console is an intentional no-op");
    owner.activate().expect("no console to reclaim");
    drop(owner);
}

fn current() -> SavedTitle {
    SavedTitle::capture().expect("read console title").expect("test owns a console")
}

fn run_child(test: &str, flags: u32) {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, "1")
        .creation_flags(flags)
        .output()
        .expect("isolated title test process");
    assert!(output.status.success(), "{output:?}");
}
