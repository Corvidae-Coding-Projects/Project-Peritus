//! Exercise disconnect and cancellation through the authenticated HTTP route and real Git hooks.
use super::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tower::ServiceExt as _;

fn app(root: &Path, state: &Path) -> Arc<App> {
    Arc::new(
        App::open(
            crate::config::Options {
                port: 4173,
                root: root.into(),
                assets: state.join("assets"),
                config_file: state.join("config.toml"),
                state_file: state.join("workspace.json"),
                daemon_config_root: state.into(),
                product_state_root: state.join("product"),
                daemon_config: None,
                endpoint: None,
                cli: std::env::current_exe().unwrap(),
            },
            4173,
        )
        .unwrap(),
    )
}
fn request(app: &App, path: &str, body: &Value) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "127.0.0.1:4173")
        .header("x-peritus-token", &app.token)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git").current_dir(root).args(args).output().unwrap();
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}
async fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "fixture transition deadline");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnected_git_keeps_mutation_custody_and_cancellation_is_reachable() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-b", "main"]);
    git(root.path(), &["config", "user.name", "Test"]);
    git(root.path(), &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.path().join("file"), "initial").unwrap();
    git(root.path(), &["add", "file"]);
    git(root.path(), &["commit", "-m", "initial"]);
    let hook = root.path().join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nprintf 'started\\n' > .git/hook-started\nwhile ! test -f .git/hook-release; do sleep 0.02; done\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    let app = app(root.path(), state.path());
    let project = app.snapshot().unwrap().projects[0].id.clone();
    let input = json!({"operation":"commit-first","command":"git","action":"commit","project":project,"message":"original after disconnect"});
    std::fs::write(root.path().join("file"), "modified").unwrap();
    git(root.path(), &["add", "file"]);
    let executing =
        tokio::spawn(router(Arc::clone(&app)).oneshot(request(&app, "/api/action", &input)));
    until(|| root.path().join(".git/hook-started").exists()).await;
    executing.abort();
    assert!(app.owned_operations.lock().unwrap().contains("commit-first"));
    assert!(operations::observe(&app, "commit-first").await.unwrap()["result"].is_null());
    assert!(operations::acknowledge(&app, "commit-first").await.is_err());
    let custody = app.lock(format!("git:{project}")).unwrap();
    assert!(custody.try_lock().is_err());
    let repeated =
        router(Arc::clone(&app)).oneshot(request(&app, "/api/action", &input)).await.unwrap();
    let bytes = axum::body::to_bytes(repeated.into_body(), 65536).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["uncertain"], true);
    std::fs::write(root.path().join(".git/hook-release"), "release").unwrap();
    until(|| app.snapshot().unwrap().operations["commit-first"].result.is_some()).await;
    assert!(custody.try_lock().is_ok());
    assert_eq!(git(root.path(), &["rev-list", "--count", "HEAD"]).trim(), "2");
    std::fs::remove_file(root.path().join(".git/hook-release")).unwrap();
    std::fs::remove_file(root.path().join(".git/hook-started")).unwrap();
    std::fs::write(root.path().join("file"), "cancel me").unwrap();
    git(root.path(), &["add", "file"]);
    let input = json!({"operation":"commit-cancel","command":"git","action":"commit","project":project,"message":"must not finish"});
    let executing =
        tokio::spawn(router(Arc::clone(&app)).oneshot(request(&app, "/api/action", &input)));
    until(|| root.path().join(".git/hook-started").exists()).await;
    let cancelled = router(Arc::clone(&app)).oneshot(request(&app, "/api/action", &json!({"operation":"cancel-control","command":"cancel-operation","original":"commit-cancel"}))).await.unwrap();
    assert!(cancelled.status().is_success());
    let result = executing.await.unwrap().unwrap();
    let bytes = axum::body::to_bytes(result.into_body(), 65536).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["uncertain"], true);
    assert_eq!(git(root.path(), &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert!(app.snapshot().unwrap().operations["commit-cancel"].result.is_none());
}

#[tokio::test]
async fn preparation_cannot_be_mistaken_for_a_crashed_unsubmitted_git_operation() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let app = app(root.path(), state.path());
    let input = json!({"command":"git","action":"commit"});
    let owner = crate::state::OperationOwner::new(&app, "preparing").unwrap();
    assert!(!app.snapshot().unwrap().operations.contains_key("preparing"));
    app.record_operation("preparing".into(), input.clone()).unwrap();
    assert!(git::recover(&app, "preparing", &input).await.unwrap().is_none());
    assert!(operations::acknowledge(&app, "preparing").await.is_err());
    drop(owner);
    let outcome = git::recover(&app, "preparing", &input).await.unwrap().unwrap();
    assert_eq!(outcome["submitted"], false);
    assert_eq!(outcome["retryable"], true);
}
