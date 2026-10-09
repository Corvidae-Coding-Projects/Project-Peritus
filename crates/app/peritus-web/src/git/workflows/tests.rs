use super::*;

async fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.name", "WebUI Test"],
        vec!["config", "user.email", "test@example.invalid"],
    ] {
        run(root.path(), &args.into_iter().map(String::from).collect::<Vec<_>>()).await.unwrap();
    }
    std::fs::write(root.path().join("file.txt"), "original\n").unwrap();
    run(root.path(), &["add".into(), ".".into()]).await.unwrap();
    run(root.path(), &["commit".into(), "-m".into(), "initial".into()]).await.unwrap();
    root
}

#[tokio::test]
async fn branch_workflows_preserve_uncommitted_changes() {
    let repo = repository().await;
    action(repo.path(), "branch-create", &json!({"branch":"feature/editor"})).await.unwrap();
    std::fs::write(repo.path().join("file.txt"), "feature\n").unwrap();
    run(repo.path(), &["commit".into(), "-am".into(), "feature".into()]).await.unwrap();
    action(
        repo.path(),
        "branch-rename",
        &json!({"branch":"feature/editor","name":"feature/review"}),
    )
    .await
    .unwrap();
    action(repo.path(), "branch-switch", &json!({"branch":"refs/heads/main"})).await.unwrap();
    std::fs::write(repo.path().join("file.txt"), "unsaved local work\n").unwrap();
    assert!(
        action(repo.path(), "branch-switch", &json!({"branch":"feature/review"})).await.is_err()
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("file.txt")).unwrap(),
        "unsaved local work\n"
    );
    assert!(
        action(repo.path(), "branch-delete", &json!({"branch":"feature/review","confirmed":true}))
            .await
            .is_err()
    );
    assert!(branch_name(repo.path(), "--discard-changes").await.is_err());
    assert!(branch_name(repo.path(), "@{-1}").await.is_err());
    let (branches, _) = inventory(repo.path()).await.unwrap();
    assert!(branches.iter().any(|branch| branch["name"] == "main" && branch["current"] == true));
}

#[tokio::test]
async fn remote_workflows_publish_fetch_track_and_remove() {
    let repo = repository().await;
    let remote = tempfile::tempdir().unwrap();
    run(remote.path(), &["init".into(), "--bare".into()]).await.unwrap();
    let url = remote.path().to_string_lossy();
    action(repo.path(), "remote-add", &json!({"remote":"upstream","url":url})).await.unwrap();
    action(repo.path(), "remote-rename", &json!({"remote":"upstream","name":"origin"}))
        .await
        .unwrap();
    action(repo.path(), "remote-url", &json!({"remote":"origin","url":url})).await.unwrap();
    action(
        repo.path(),
        "push",
        &json!({"remote":"origin","branch":"published","setUpstream":true}),
    )
    .await
    .unwrap();
    action(repo.path(), "fetch", &json!({"remote":"origin"})).await.unwrap();
    action(repo.path(), "branch-switch", &json!({"branch":"refs/remotes/origin/published"}))
        .await
        .unwrap();
    let (branches, remotes) = inventory(repo.path()).await.unwrap();
    assert_eq!(remotes[0]["name"], "origin");
    assert!(branches.iter().any(|branch| branch["name"] == "published" && branch["upstream"] == "origin/published"));
    assert!(action(repo.path(), "remote-remove", &json!({"remote":"origin"})).await.is_err());
    action(repo.path(), "remote-remove", &json!({"remote":"origin","confirmed":true}))
        .await
        .unwrap();
    assert!(inventory(repo.path()).await.unwrap().1.is_empty());
    assert!(remote_url("ext::sh -c malicious").is_err());
    assert!(remote_url("--upload-pack=command").is_err());
}
