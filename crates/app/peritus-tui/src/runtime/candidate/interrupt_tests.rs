use super::*;
use std::{
    io::{BufReader, Read as _},
    os::unix::process::CommandExt as _,
    process::Stdio,
};

const FIXTURE: &str = "runtime::candidate::interrupt_tests::foreground_interrupt_fixture";

#[test]
fn foreground_interrupt_returns_to_the_client_instead_of_terminating_it() {
    interrupt_client(false);
}

#[test]
fn os_interrupt_after_foreground_exit_quits_the_resumed_ui() {
    interrupt_client(true);
}

fn interrupt_client(check_ui: bool) {
    let workspace = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", FIXTURE, "--ignored", "--nocapture"])
        .env("PERITUS_TUI_INTERRUPT_WORKSPACE", workspace.path())
        .env("PERITUS_TUI_INTERRUPT_CHECK_UI", if check_ui { "1" } else { "0" })
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut captured = String::new();
    read_until(&mut output, &mut captured, "peritus-candidate-ready");
    let group = nix::unistd::Pid::from_raw(i32::try_from(child.id()).unwrap());
    nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGINT).unwrap();
    read_until(&mut output, &mut captured, "peritus-client-survived");
    if check_ui {
        nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGINT).unwrap();
    }
    output.read_to_string(&mut captured).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "interrupt killed the TUI owner: {status}; {captured}");
    if check_ui {
        assert!(captured.contains("peritus-ui-quit"), "{captured}");
    }
}

fn read_until(output: &mut impl std::io::BufRead, captured: &mut String, marker: &str) {
    loop {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "fixture never reached {marker}: {captured}"
        );
        captured.push_str(&line);
        if line.contains(marker) {
            break;
        }
    }
}

#[tokio::test]
#[ignore = "isolated subprocess fixture receives a real foreground process-group interrupt"]
async fn foreground_interrupt_fixture() {
    let mut interrupts = super::super::interrupts::listen().unwrap();
    let workspace = PathBuf::from(
        std::env::var_os("PERITUS_TUI_INTERRUPT_WORKSPACE").expect("fixture workspace"),
    );
    for args in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(Command::new("git").args(args).current_dir(&workspace).status().unwrap().success());
    }
    std::fs::write(
        workspace.join("run.sh"),
        b"echo peritus-candidate-ready
exec sleep 30
",
    )
    .unwrap();
    let digest = ProductRunner::candidate_digest(&workspace).unwrap();
    let result = execute(workspace, "sh run.sh".into(), digest, &mut interrupts).await;
    let error = result.expect_err("interrupted child must not report success");
    assert!(error.contains("SIGINT"), "child must actually terminate from the interrupt: {error}");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), interrupts.recv())
            .await
            .is_err(),
        "foreground interrupt leaked into the resumed UI"
    );
    let mut reads = super::super::LocalReads { interrupts: Some(interrupts), ..Default::default() };
    let (_input, mut input_rx) = tokio::sync::mpsc::channel(1);
    let (events, mut event_rx) = tokio::sync::mpsc::channel(1);
    let mut connection = super::super::Connection::new(events);
    let mut model = crate::model::AppModel::new([94; 32]);
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(25));
    println!("peritus-client-survived");
    if std::env::var("PERITUS_TUI_INTERRUPT_CHECK_UI").unwrap_or_default() != "1" {
        return;
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let effects = super::super::next_effects(
                &mut model,
                &mut input_rx,
                &mut event_rx,
                &mut tick,
                &mut reads,
                &mut connection,
            )
            .await
            .unwrap();
            if effects.iter().any(|effect| matches!(effect, crate::action::Effect::Quit)) {
                break;
            }
        }
    })
    .await
    .expect("an interrupt while the UI is active must still quit it");
    println!("peritus-ui-quit");
}
