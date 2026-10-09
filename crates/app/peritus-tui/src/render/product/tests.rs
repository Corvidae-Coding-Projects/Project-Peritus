use peritus_app_protocol::{
    ProductDeliverable, ProductProviderSelection, ProductRunLegalControls, ProductRunOperation,
    ProductRunOperationKind, ProductRunOperationState,
};
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
        crate::test_support::run_operation(RunId::new([4; 16]).unwrap(), ProductRunPhase::Complete),
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
fn candidate_detail_names_dependency_axes_and_unobserved_execution_context() {
    let (run, settlement) = dependency_candidate("remaining work");

    let rendered = run_detail(&run, Some(&settlement), None)
        .lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<String>();
    assert!(rendered.contains("Content"));
    assert!(rendered.contains("Repository context"));
    assert!(rendered.contains("Requirements revision7"));
    assert!(rendered.contains("Execution context not observed"), "{rendered:?}");
    assert!(rendered.contains("stale · content + requirements + execution"));
    assert_eq!(rendered.matches("passed · content + requirements").count(), 2);
}

#[test]
fn dashboard_paging_makes_every_candidate_dependency_reachable() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let summary =
        "Writer completed the requested behavior and preserved every unrelated file. ".repeat(20);
    let (run, settlement) = dependency_candidate(&summary);
    let launch = crate::runtime::ProductLaunchContext::new(
        run.workspace_id(),
        "fixture".to_owned(),
        vec![crate::runtime::ProductProviderOption::new(run.providers().writer(), "fixture")],
        Some(0),
    )
    .expect("launch");
    let mut model = AppModel::with_product([19; 32], Some(launch));
    model.view = crate::model::View::Runs;
    model.chat.viewport = Some(Rect::new(0, 0, 80, 24));
    let product = model.product.as_mut().expect("product");
    product.settlements.insert(run.run_id(), settlement);
    product.runs.push(run);

    let maximum = crate::render::inspection_scroll_limit(&model);
    assert!(maximum > 0, "fixture must overflow the Progress pane");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    let mut visible = String::new();
    let final_screen;
    loop {
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let screen =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
        visible.push_str(&screen);
        let current = model.product.as_ref().expect("product").detail_scroll;
        if current == maximum {
            final_screen = screen;
            break;
        }
        let _ = model.update(crate::action::Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::PageDown,
            KeyModifiers::NONE,
        ))));
        assert!(model.product.as_ref().expect("product").detail_scroll > current);
    }
    assert!(final_screen.contains("State"), "End must not render a blank tail: {final_screen}");
    for expected in [
        "Content",
        "Repository context",
        "Requirements revision",
        "Execution context",
        "Checks",
        "Requirements",
        "Review",
        "State",
    ] {
        assert!(visible.contains(expected), "missing {expected}: {visible}");
    }
}

fn dependency_candidate(
    summary: &str,
) -> (ProductRunSnapshot, peritus_run_settlement::RunSettlement) {
    use peritus_run_settlement::{
        CandidateCheckpoint, CandidateIdentity, EvidenceDependencies, EvidenceRecord,
        EvidenceStatus, QualificationEvidence, SettlementCause, SettlementReducer,
    };
    use peritus_types::Sha256Digest;

    let source_run = candidate_snapshot(ProductRunPhase::Failed);
    let observed = CandidateIdentity::new(
        source_run.run_id(),
        source_run.workspace_id(),
        Sha256Digest::new([4; 32]),
        Sha256Digest::new([5; 32]),
        Some(Sha256Digest::new([6; 32])),
        7,
        1,
    )
    .expect("observed identity");
    let current = CandidateIdentity::new(
        source_run.run_id(),
        source_run.workspace_id(),
        observed.content_digest(),
        observed.repository_digest(),
        None,
        7,
        2,
    )
    .expect("current identity");
    let gate = EvidenceRecord::new(
        observed,
        EvidenceDependencies::GATES,
        QualificationEvidence::Satisfied,
    );
    let source = EvidenceRecord::new(
        observed,
        EvidenceDependencies::OBLIGATIONS,
        QualificationEvidence::Satisfied,
    );
    let checkpoint = CandidateCheckpoint::new(
        current,
        CandidateStage::SelfChecked,
        EvidenceStatus::Stale(gate),
        EvidenceStatus::Current(source),
        EvidenceStatus::Current(source),
    )
    .expect("checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observation");
    let settlement = reducer.settle(SettlementCause::Provider).expect("settlement");
    let source_deliverable = source_run.deliverable().expect("deliverable");
    let deliverable = ProductDeliverable::candidate(
        source_deliverable.workspace_path().to_owned(),
        source_deliverable.changed_paths().to_vec(),
        source_deliverable.successful_commands().to_vec(),
        source_deliverable.run_instructions().to_owned(),
        CandidateStage::SelfChecked,
    )
    .expect("self-checked deliverable");
    let run = ProductRunSnapshot::new(
        source_run.run_id(),
        source_run.workspace_id(),
        source_run.providers(),
        source_run.phase(),
        source_run.cycle(),
        source_run.task().to_owned(),
        source_run.status().to_owned(),
        source_run.diff().to_owned(),
        source_run.gates().to_owned(),
        source_run.review().to_owned(),
        summary.to_owned(),
        crate::test_support::run_operation(source_run.run_id(), source_run.phase()),
    )
    .expect("snapshot")
    .with_operation(source_run.operation().clone())
    .with_deliverable(deliverable);
    (run, settlement)
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
    for (offset, expected) in [(0, "Operation"), (u16::MAX, "diff --git")] {
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
        crate::test_support::run_operation(stopped.run_id(), ProductRunPhase::Failed),
    )
    .expect("stopped snapshot")
    .with_operation(operation_for(ProductRunPhase::Failed));
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
        crate::test_support::run_operation(RunId::new([2; 16]).expect("run"), phase),
    )
    .expect("snapshot")
    .with_operation(operation_for(phase))
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

fn operation_for(phase: ProductRunPhase) -> ProductRunOperation {
    let state = match phase {
        ProductRunPhase::Queued
        | ProductRunPhase::Designing
        | ProductRunPhase::Writing
        | ProductRunPhase::Checking
        | ProductRunPhase::Reviewing
        | ProductRunPhase::Fixing
        | ProductRunPhase::Verifying => ProductRunOperationState::Running,
        ProductRunPhase::WaitingForUser => ProductRunOperationState::WaitingForUser,
        ProductRunPhase::Complete => ProductRunOperationState::Succeeded,
        ProductRunPhase::Failed => ProductRunOperationState::Failed,
        ProductRunPhase::Cancelled => ProductRunOperationState::Cancelled,
        ProductRunPhase::RecoveryRequired => ProductRunOperationState::RecoveryRequired,
    };
    ProductRunOperation::new(
        ProductRunOperationKind::Execution,
        state,
        "run/test".to_owned(),
        "The daemon projected this exact operation.".to_owned(),
        String::new(),
        ProductRunLegalControls::none(),
    )
    .expect("operation")
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
