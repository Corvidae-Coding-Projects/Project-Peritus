//! Owned subprocess validation, bounds, cancellation, and alternate-transport seam.

#[path = "support/runtime.rs"]
mod runtime;

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::time::Duration;

use peritus_provider_core::{
    BoxFuture, CancellationToken, EnvironmentName, ProcessExecutable, ProcessExit, ProcessLimits,
    ProcessOutput, ProcessRequest, ProcessTransport, ProviderCoreError, ProviderCoreErrorKind,
    TokioProcessTransport,
};

fn limits(stdout: usize, timeout: Duration) -> ProcessLimits {
    ProcessLimits::new(1_024, stdout, 1_024, timeout).expect("process limits")
}

fn environment(name: &str) -> EnvironmentName {
    EnvironmentName::new(name.to_owned()).expect("environment name")
}

fn current_test_executable() -> ProcessExecutable {
    ProcessExecutable::pin(std::env::current_exe().expect("current test executable"))
        .expect("pinned test executable")
}

#[test]
fn tokio_transport_applies_argv_stdin_cwd_and_environment_removals() {
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        std::fs::write(directory.path().join("process-helper.marker"), b"owned")
            .expect("helper marker");
        let request = ProcessRequest::new(
            current_test_executable(),
            helper_arguments(),
            b"echo\nsensitive-stdin".to_vec(),
            Some(directory.path().to_path_buf()),
            vec![environment("PATH")],
            limits(4_096, Duration::from_secs(10)),
        )
        .expect("request");
        assert!(!format!("{request:?}").contains("sensitive-stdin"));
        let output = TokioProcessTransport
            .run(request, &CancellationToken::new())
            .await
            .expect("process output");
        assert!(output.exit().success());
        let stdout = std::str::from_utf8(output.stdout()).expect("UTF-8 stdout");
        assert!(stdout.contains("HELPER|cwd="), "unexpected stdout: {stdout:?}");
        assert!(stdout.contains("|path=false|stdin=sensitive-stdin|"));
        assert!(stdout.contains("--exact"));
        assert!(output.stderr().is_empty());
        assert!(!format!("{output:?}").contains("sensitive-stdin"));
    });
}

#[test]
fn output_limit_and_cancellation_terminate_and_observe_the_child() {
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        std::fs::write(directory.path().join("process-helper.marker"), b"owned")
            .expect("helper marker");
        let request = ProcessRequest::new(
            current_test_executable(),
            helper_arguments(),
            b"excessive".to_vec(),
            Some(directory.path().to_path_buf()),
            Vec::new(),
            limits(4, Duration::from_secs(10)),
        )
        .expect("request");
        let error = TokioProcessTransport
            .run(request, &CancellationToken::new())
            .await
            .expect_err("stdout limit");
        assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);

        let request = ProcessRequest::new(
            current_test_executable(),
            helper_arguments(),
            b"spin".to_vec(),
            Some(directory.path().to_path_buf()),
            Vec::new(),
            limits(32, Duration::from_secs(10)),
        )
        .expect("request");
        let cancellation = CancellationToken::new();
        let signal = cancellation.clone();
        let owner = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert!(signal.cancel());
        });
        let error =
            TokioProcessTransport.run(request, &cancellation).await.expect_err("cancelled process");
        owner.await.expect("cancellation owner");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Cancelled);
    });
}

fn helper_arguments() -> Vec<String> {
    vec!["--exact".to_owned(), "process_transport_helper".to_owned(), "--nocapture".to_owned()]
}

#[test]
fn process_transport_helper() {
    if !std::path::Path::new("process-helper.marker").is_file() {
        return;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("helper stdin");
    let (mode, payload) = input.split_once('\n').unwrap_or((&input, ""));
    match mode {
        "echo" => {
            let mut stdout = std::io::stdout();
            let mut output = String::new();
            write!(
                output,
                "HELPER|cwd={}|path={}|stdin={payload}|args={:?}",
                std::env::current_dir().expect("helper cwd").display(),
                std::env::var_os("PATH").is_some(),
                std::env::args().collect::<Vec<_>>(),
            )
            .expect("helper output");
            stdout.write_all(output.as_bytes()).expect("helper output");
            stdout.write_all(b"\n").expect("helper newline");
        }
        "excessive" => std::io::stdout().write_all(b"0123456789").expect("helper output"),
        "spin" => loop {
            std::hint::spin_loop();
        },
        _ => panic!("unknown helper mode"),
    }
}

struct FakeTransport;

impl ProcessTransport for FakeTransport {
    fn run<'a>(
        &'a self,
        request: ProcessRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ProcessOutput, ProviderCoreError>> {
        Box::pin(async move {
            assert_eq!(request.arguments(), ["fake"]);
            assert_eq!(request.stdin(), b"input");
            assert_eq!(request.environment_removals(), [environment("TOKEN")]);
            ProcessOutput::new(
                ProcessExit::new(true, Some(0)),
                b"output".to_vec(),
                Vec::new(),
                request.limits(),
            )
        })
    }
}

#[test]
fn public_fake_seam_observes_owned_values_without_tokio_types() {
    runtime::block_on(async {
        let executable = current_test_executable();
        let request = ProcessRequest::new(
            executable,
            vec!["fake".to_owned()],
            b"input".to_vec(),
            None,
            vec![environment("TOKEN")],
            limits(32, Duration::from_secs(1)),
        )
        .expect("request");
        let output =
            FakeTransport.run(request, &CancellationToken::new()).await.expect("fake output");
        assert_eq!(output.stdout(), b"output");
    });
}

#[test]
fn request_validation_rejects_duplicates_and_invalid_limits() {
    let executable = current_test_executable();
    let error = ProcessRequest::new(
        executable,
        Vec::new(),
        Vec::new(),
        None,
        vec![environment("TOKEN"), environment("TOKEN")],
        limits(32, Duration::from_secs(1)),
    )
    .expect_err("duplicate environment removal");
    assert_eq!(error.kind(), ProviderCoreErrorKind::Configuration);
    assert!(ProcessLimits::new(0, 1, 1, Duration::from_secs(1)).is_err());
}
