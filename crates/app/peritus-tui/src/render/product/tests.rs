use peritus_app_protocol::{ProductDeliverable, ProductProviderSelection};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::*;

#[test]
fn candidate_inspection_snapshot_names_every_handoff_field() {
    let run = candidate_snapshot(ProductRunPhase::Failed);

    assert_eq!(product_state(&run), "Candidate available");
    let text = inspect_text(&run);
    assert!(text.contains("Workspace\n/managed/tetris"));
    assert!(text.contains("Exact candidate paths\ngame/src/main.rs\ngame/Cargo.toml"));
    assert!(text.contains("Successful commands\ncargo test"));
    assert!(text.contains("Run instructions\ncargo run"));
    assert!(text.contains("Diff\ndiff --git"));
}

#[test]
fn terminal_state_snapshot_distinguishes_each_user_outcome() {
    assert_eq!(product_state(&candidate_snapshot(ProductRunPhase::Complete)), "Accepted");
    assert_eq!(
        product_state(&candidate_snapshot(ProductRunPhase::WaitingForUser)),
        "Waiting for you",
    );
    assert_eq!(
        product_state(&candidate_snapshot(ProductRunPhase::RecoveryRequired)),
        "Recovery required",
    );
    assert_eq!(
        product_state(&candidate_snapshot(ProductRunPhase::Cancelled)),
        "Cancelled — candidate available",
    );
    let mut stopped = candidate_snapshot(ProductRunPhase::Failed);
    stopped = ProductRunSnapshot::new(
        stopped.run_id(),
        stopped.workspace_id(),
        stopped.providers(),
        ProductRunPhase::Failed,
        1,
        stopped.task().to_owned(),
        stopped.status().to_owned(),
        stopped.diff().to_owned(),
        stopped.gates().to_owned(),
        stopped.review().to_owned(),
        stopped.summary().to_owned(),
    )
    .expect("stopped snapshot");
    assert_eq!(product_state(&stopped), "Stopped with no candidate");
}

fn candidate_snapshot(phase: ProductRunPhase) -> ProductRunSnapshot {
    let profile = ProviderProfileId::new([1; 16]).expect("provider");
    ProductRunSnapshot::new(
        RunId::new([2; 16]).expect("run"),
        WorkspaceId::new([3; 16]).expect("workspace"),
        ProductProviderSelection::new(profile, profile, profile),
        phase,
        1,
        "build tetris".to_owned(),
        "candidate".to_owned(),
        "diff --git".to_owned(),
        "cargo test failed".to_owned(),
        "review missing".to_owned(),
        "remaining work".to_owned(),
    )
    .expect("snapshot")
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/tetris".to_owned(),
            vec!["game/src/main.rs".to_owned(), "game/Cargo.toml".to_owned()],
            vec!["cargo test".to_owned()],
            "cargo run".to_owned(),
            CandidateStage::Changed,
        )
        .expect("deliverable"),
    )
}
