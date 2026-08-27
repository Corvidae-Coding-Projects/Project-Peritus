//! Black-box exclusive daemon-instance ownership tests.

mod support;

use std::time::Duration;

use peritus_app_protocol::ShutdownCompletionDisposition;
use peritus_daemon::{DaemonErrorCode, DaemonRecovery, DaemonRuntime};
use tempfile::TempDir;

const TEST_BOUND: Duration = Duration::from_secs(10);

#[tokio::test]
async fn second_runtime_cannot_acquire_a_live_state_root() {
    let temporary = TempDir::new().expect("temporary root");
    let config = support::configuration(temporary.path());
    let owner = tokio::time::timeout(TEST_BOUND, DaemonRuntime::start(config.clone()))
        .await
        .expect("first startup completes within the bound")
        .expect("first daemon owns state root");

    let second = tokio::time::timeout(TEST_BOUND, DaemonRuntime::start(config))
        .await
        .expect("second startup attempt completes within the bound");
    let error = match second {
        Ok(_) => panic!("second daemon unexpectedly acquired the live state root"),
        Err(error) => error,
    };
    assert_eq!(error.code_kind(), DaemonErrorCode::AlreadyRunning);
    assert_eq!(error.recovery(), DaemonRecovery::Retry);

    let outcome = tokio::time::timeout(TEST_BOUND, owner.shutdown())
        .await
        .expect("owner shutdown completes within the bound")
        .expect("owner shuts down cleanly");
    assert_eq!(outcome.disposition(), ShutdownCompletionDisposition::Clean);
    assert!(outcome.remaining().is_empty());
    assert!(outcome.failures().is_empty());
}
