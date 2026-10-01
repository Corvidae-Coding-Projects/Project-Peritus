use peritus_app_protocol::{ProductDeliverable, ProductProviderSelection};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::*;

#[test]
fn dashboard_renders_a_multibyte_task_at_the_summary_cutoff() {
    let provider = ProviderProfileId::new([2; 16]).unwrap();
    let workspace = WorkspaceId::new([3; 16]).unwrap();
    let launch = crate::runtime::ProductLaunchContext::new(
        workspace,
        "fixture".into(),
        vec![crate::runtime::ProductProviderOption::new(provider, "fixture")],
        Some(0),
    )
    .unwrap();
    let mut model = AppModel::with_product([9; 32], Some(launch));
    let run = ProductRunSnapshot::new(
        RunId::new([4; 16]).unwrap(),
        workspace,
        ProductProviderSelection::new(provider, provider, provider),
        ProductRunPhase::Complete,
        1,
        format!("{}界 rest of task", "a".repeat(41)),
        "done".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    model.product.as_mut().unwrap().runs.push(run);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 38)).unwrap();
    let frame = terminal.draw(|frame| dashboard(frame, frame.area(), &model)).unwrap();
    let rendered =
        frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(rendered.contains(&format!("{}…", "a".repeat(41))));
}

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
fn candidate_inspection_scroll_reaches_the_diff_without_a_blank_tail() {
    use crate::runtime::{ProductLaunchContext, ProductProviderOption};
    use ratatui::{Terminal, backend::TestBackend};
    let run = candidate_snapshot(ProductRunPhase::Complete);
    let launch = ProductLaunchContext::new(
        run.workspace_id(),
        "fixture".to_owned(),
        vec![ProductProviderOption::new(run.providers().writer(), "fixture")],
        Some(0),
    )
    .expect("launch");
    let mut model = AppModel::with_product([91; 32], Some(launch));
    model.product.as_mut().expect("product").runs.push(run);
    let mut terminal = Terminal::new(TestBackend::new(60, 10)).expect("terminal");
    for (offset, expected) in [(0, "Workspace"), (u16::MAX, "diff --git")] {
        model.product.as_mut().expect("product").inspection_scroll = offset;
        let frame = terminal.draw(|frame| diff(frame, frame.area(), &model)).expect("draw");
        let text =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

#[test]
fn terminal_state_snapshot_distinguishes_each_user_outcome() {
    let complete = candidate_snapshot(ProductRunPhase::Complete);
    assert_eq!(product_state(&complete), "Ready for inspection");
    let accepted = complete.deliverable().unwrap().clone().mark_accepted();
    assert_eq!(product_state(&complete.clone().with_deliverable(accepted.clone())), "Accepted");
    let committed = accepted.mark_committed("a".repeat(40)).unwrap();
    assert_eq!(product_state(&complete.with_deliverable(committed)), "Committed");
    for phase in [ProductRunPhase::Complete, ProductRunPhase::Failed] {
        let run = candidate_snapshot(phase);
        let discarded = run.deliverable().unwrap().clone().mark_discarded();
        assert_eq!(product_state(&run.with_deliverable(discarded)), "Discarded");
    }
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
    candidate_snapshot_with_status(phase, "candidate")
}

fn candidate_snapshot_with_status(phase: ProductRunPhase, status: &str) -> ProductRunSnapshot {
    let profile = ProviderProfileId::new([1; 16]).expect("provider");
    ProductRunSnapshot::new(
        RunId::new([2; 16]).expect("run"),
        WorkspaceId::new([3; 16]).expect("workspace"),
        ProductProviderSelection::new(profile, profile, profile),
        phase,
        1,
        "build tetris".to_owned(),
        status.to_owned(),
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

#[test]
fn recovery_location_is_reachable_in_scrollable_candidate_inspection() {
    use crate::runtime::{ProductLaunchContext, ProductProviderOption};
    use ratatui::{Terminal, backend::TestBackend};
    let run = candidate_snapshot_with_status(
        ProductRunPhase::Complete,
        "Deliverable discarded\nRepository recovery: /saved/repository-recovery-123",
    );
    assert!(inspect_text(&run).contains("Status\nDeliverable discarded\nRepository recovery:"));
    let launch = ProductLaunchContext::new(
        run.workspace_id(),
        "fixture".to_owned(),
        vec![ProductProviderOption::new(run.providers().writer(), "fixture")],
        Some(0),
    )
    .unwrap();
    let mut model = AppModel::with_product([92; 32], Some(launch));
    model.product.as_mut().unwrap().runs.push(run);
    let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut reachable = false;
    for offset in 0..20 {
        model.product.as_mut().unwrap().inspection_scroll = offset;
        let frame = terminal.draw(|frame| diff(frame, frame.area(), &model)).unwrap();
        let text =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
        reachable |= text.contains("/saved/repository-recovery-123");
    }
    assert!(reachable, "recovery path must remain accessible beyond the run summary");
}
