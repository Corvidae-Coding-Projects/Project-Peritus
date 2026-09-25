//! Visible provider progress and busy-period lifecycle.
use super::*;

#[test]
fn provider_summary_is_visible_in_the_default_transcript() {
    let mut model = model();
    let previous = model.chat.snapshot.take().unwrap();
    model.chat.snapshot = Some(
        ProductInteractionSnapshot::new(
            previous.snapshot().clone(),
            previous.mode(),
            previous.models().clone(),
            previous.received(),
            previous.incorporated(),
            vec![
                ProductActivity::new(
                    1,
                    ProductActivityKind::Status,
                    "Comparing the two implementations".to_owned(),
                    "Provider thinking summary".to_owned(),
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap(),
    );
    assert!(!model.chat.expanded);
    let (text, _) = screen(&model, 100, 24);
    assert!(text.contains("Thinking"));
    assert!(text.contains("Comparing the two implementations"));
}

#[test]
fn working_idler_stops_at_every_idle_or_terminal_phase() {
    use crate::action::Action;
    use std::time::{Duration, Instant};
    for phase in [
        ProductRunPhase::WaitingForUser,
        ProductRunPhase::Complete,
        ProductRunPhase::Failed,
        ProductRunPhase::Cancelled,
        ProductRunPhase::RecoveryRequired,
    ] {
        let mut model = model();
        let now = Instant::now();
        let _ = model.update(Action::Tick(now));
        assert_eq!(model.chat.working.elapsed_seconds(), Some(0));
        let previous = model.chat.snapshot.take().expect("snapshot");
        let current = previous.snapshot();
        let snapshot = ProductRunSnapshot::new(
            current.run_id(),
            current.workspace_id(),
            current.providers(),
            phase,
            current.cycle(),
            current.task().to_owned(),
            "Stopped".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .expect("phase snapshot");
        model.chat.snapshot = Some(
            ProductInteractionSnapshot::new(
                snapshot,
                previous.mode(),
                previous.models().clone(),
                previous.received(),
                previous.incorporated(),
                previous.activities().to_vec(),
                None,
            )
            .expect("interaction"),
        );
        let _ = model.update(Action::Tick(now + Duration::from_secs(40)));
        assert_eq!(model.chat.working.elapsed_seconds(), None, "{phase:?}");
        assert!(!screen(&model, 100, 32).0.contains("*working"));
    }
}
