use std::{
    cell::Cell,
    collections::VecDeque,
    ffi::{OsStr, OsString},
    future,
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
    sync::Arc,
};

use super::{AttemptOutcome, StopIntent, control_paths, serve_command, supervise};

#[test]
fn recovery_continues_past_five_failures_but_not_an_unclean_intentional_stop() {
    let runtime = test_runtime();
    let outcomes = Rc::new(std::cell::RefCell::new(VecDeque::from([
        AttemptOutcome::Failed("fixture-1".to_owned()),
        AttemptOutcome::Failed("fixture-2".to_owned()),
        AttemptOutcome::Failed("fixture-3".to_owned()),
        AttemptOutcome::Failed("fixture-4".to_owned()),
        AttemptOutcome::Failed("fixture-5".to_owned()),
        AttemptOutcome::Failed("fixture-6".to_owned()),
        AttemptOutcome::Stopped(ExitCode::FAILURE),
    ])));
    let attempts = Rc::new(Cell::new(0));
    let delays = Rc::new(Cell::new(0));
    let stop = Arc::new(StopIntent::new());

    let result = runtime.block_on(supervise(
        stop,
        {
            let outcomes = Rc::clone(&outcomes);
            let attempts = Rc::clone(&attempts);
            move || {
                attempts.set(attempts.get() + 1);
                future::ready(outcomes.borrow_mut().pop_front().expect("fixture outcome"))
            }
        },
        {
            let delays = Rc::clone(&delays);
            move || {
                delays.set(delays.get() + 1);
                future::ready(())
            }
        },
        |_| {},
    ));

    assert_eq!(result, ExitCode::FAILURE);
    assert_eq!(attempts.get(), 7);
    assert_eq!(delays.get(), 6);
}

#[test]
fn stop_intent_during_retry_delay_prevents_another_attempt() {
    let runtime = test_runtime();
    let stop = Arc::new(StopIntent::new());
    let attempts = Rc::new(Cell::new(0));
    let delays = Rc::new(Cell::new(0));

    let result = runtime.block_on(supervise(
        Arc::clone(&stop),
        {
            let attempts = Rc::clone(&attempts);
            move || {
                attempts.set(attempts.get() + 1);
                future::ready(AttemptOutcome::Failed("fixture".to_owned()))
            }
        },
        {
            let stop = Arc::clone(&stop);
            let delays = Rc::clone(&delays);
            move || {
                delays.set(delays.get() + 1);
                stop.request();
                future::ready(())
            }
        },
        |_| {},
    ));

    assert_eq!(result, ExitCode::SUCCESS);
    assert_eq!(attempts.get(), 1);
    assert_eq!(delays.get(), 1);
}

#[test]
fn stop_intent_before_start_prevents_the_first_attempt() {
    let runtime = test_runtime();
    let stop = Arc::new(StopIntent::new());
    stop.request();
    let attempts = Cell::new(0);

    let result = runtime.block_on(supervise(
        stop,
        || {
            attempts.set(attempts.get() + 1);
            future::ready(AttemptOutcome::Stopped(ExitCode::SUCCESS))
        },
        || future::ready(()),
        |_| {},
    ));

    assert_eq!(result, ExitCode::SUCCESS);
    assert_eq!(attempts.get(), 0);
}

#[test]
fn supervised_child_receives_only_internal_serve_and_exact_control_paths() {
    #[cfg(windows)]
    const EXECUTABLE_NAME: &str = "peritusd.exe";
    #[cfg(not(windows))]
    const EXECUTABLE_NAME: &str = "peritusd";
    let temporary = tempfile::tempdir().expect("temporary directory");
    let configuration = temporary.path().join("peritus config.toml");
    let executable = temporary.path().join(EXECUTABLE_NAME);
    let token = OsStr::new("fixture-owner-token");
    let controls = control_paths(configuration.as_os_str()).expect("control paths");
    let command = serve_command(&executable, configuration.as_os_str(), token);

    assert_eq!(command.get_program(), executable.as_os_str());
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            OsStr::new("--supervised-serve-v1"),
            OsStr::new("--config"),
            configuration.as_os_str(),
            OsStr::new("--owner-token"),
            token,
        ]
    );
    assert_eq!(controls.lock, suffixed(&configuration, ".supervisor.lock"));
    assert_eq!(controls.owner, suffixed(&configuration, ".supervisor.owner"));
    assert_eq!(controls.stop, suffixed(&configuration, ".supervisor.stop"));
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn test_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime")
}
