use super::*;

#[test]
fn historical_snapshot_preserves_context_but_holds_all_inherited_pending_work() {
    let original = enqueue(&InputLedger::default(), 1, Vec::new());
    let original = apply(&original, &incorporate(vec![selected(1, 1)])).unwrap();
    let original = apply(
        &original,
        &QueueIntent::Correct {
            original: selected(1, 1),
            id: id(2),
            text: text("pending correction"),
        },
    )
    .unwrap();
    let original = enqueue(&original, 3, vec![id(2)]);
    let original =
        apply(&original, &QueueIntent::Hold { selected: selected(3, 1), held: true }).unwrap();
    let original = enqueue(&original, 4, Vec::new());
    let original = apply(&original, &QueueIntent::Withdraw(selected(4, 1))).unwrap();
    let snapshot = original.historical_snapshot();
    snapshot.validate().unwrap();
    assert!(snapshot.capture().unwrap().pending().is_empty());
    assert_eq!(snapshot.order(), original.order());
    assert_eq!(snapshot.invocations(), original.invocations());
    for (source, child) in original.revisions.iter().zip(&snapshot.revisions) {
        assert_eq!(source.selection(), child.selection());
        assert_eq!(source.text(), child.text());
        assert_eq!(source.correction_of(), child.correction_of());
        assert_eq!(
            child.state(),
            if source.state() == InputState::Queued { InputState::Held } else { source.state() }
        );
    }
    assert_eq!(original.latest(id(2)).unwrap().state(), InputState::Queued);
    let released =
        apply(&snapshot, &QueueIntent::Hold { selected: selected(2, 1), held: false }).unwrap();
    assert_eq!(released.capture().unwrap().pending(), &[selected(2, 1)]);
}

fn id(byte: u8) -> InputId {
    InputId::new([byte; 16]).expect("input")
}
fn selected(byte: u8, revision: u64) -> InputSelection {
    InputSelection::new(id(byte), revision).expect("selection")
}
fn text(value: &str) -> ControlText<8192> {
    ControlText::new(value.to_owned()).expect("text")
}
fn apply(ledger: &InputLedger, intent: &QueueIntent) -> Result<InputLedger, ControlError> {
    ledger.apply(peritus_types::ActorId::new([21; 16]).expect("actor"), intent)
}
fn enqueue(ledger: &InputLedger, byte: u8, dependencies: Vec<InputId>) -> InputLedger {
    apply(
        ledger,
        &QueueIntent::Enqueue { id: id(byte), text: text("original input"), dependencies },
    )
    .expect("enqueue")
}
fn incorporate(items: Vec<InputSelection>) -> QueueIntent {
    QueueIntent::Incorporate {
        invocation: InvocationId::new([22; 16]).expect("invocation"),
        request_digest: [23; 32],
        manifest_digest: [24; 32],
        items,
    }
}

#[test]
fn editing_preserves_old_text_and_incorporated_text_requires_a_new_correction() {
    let original = enqueue(&InputLedger::default(), 1, Vec::new());
    let edited = apply(
        &original,
        &QueueIntent::Edit { selected: selected(1, 1), text: text("corrected before use") },
    )
    .expect("edit");
    assert_eq!(edited.revisions[0].state, InputState::Superseded);
    assert_eq!(edited.revisions[0].text(), "original input");
    assert_eq!(edited.revisions[1].selection(), selected(1, 2));
    assert!(matches!(
        apply(&edited, &QueueIntent::Withdraw(selected(1, 1))),
        Err(ControlError::StaleRevision)
    ));
    let bound = apply(&edited, &incorporate(vec![selected(1, 2)])).expect("incorporate");
    assert_eq!(bound.invocations()[0].items(), &[selected(1, 2)]);
    assert_eq!(bound.invocations()[0].request_digest().as_bytes(), &[23; 32]);
    assert!(bound.order().is_empty());
    assert!(
        apply(
            &bound,
            &QueueIntent::Edit { selected: selected(1, 2), text: text("rewrite history") }
        )
        .is_err()
    );
    let corrected = apply(
        &bound,
        &QueueIntent::Correct {
            original: selected(1, 2),
            id: id(2),
            text: text("correction after use"),
        },
    )
    .expect("new correction");
    assert_eq!(corrected.latest(id(1)).expect("old").text(), "corrected before use");
    assert_eq!(corrected.latest(id(1)).expect("old").state(), InputState::Incorporated);
    assert_eq!(corrected.latest(id(2)).expect("new").correction_of(), Some(selected(1, 2)));
    assert_eq!(corrected.order(), &[id(2)]);
    assert_eq!(corrected.generation(), 3);
}

#[test]
fn dependency_order_hold_withdraw_and_incorporation_reject_invalid_plans_without_mutation() {
    let ledger = enqueue(&InputLedger::default(), 1, Vec::new());
    let ledger = enqueue(&ledger, 2, vec![id(1)]);
    for intent in [
        QueueIntent::Reorder(vec![id(2), id(1)]),
        QueueIntent::Reorder(vec![id(1), id(1)]),
        QueueIntent::Withdraw(selected(1, 1)),
        incorporate(vec![selected(2, 1)]),
        incorporate(vec![selected(2, 1), selected(1, 1)]),
        incorporate(vec![selected(1, 1), selected(1, 1)]),
    ] {
        assert!(apply(&ledger, &intent).is_err(), "{intent:?}");
    }
    let held =
        apply(&ledger, &QueueIntent::Hold { selected: selected(1, 1), held: true }).expect("hold");
    assert!(apply(&held, &incorporate(vec![selected(1, 1), selected(2, 1)])).is_err());
    let bound =
        apply(&ledger, &incorporate(vec![selected(1, 1), selected(2, 1)])).expect("bind both");
    assert_eq!(bound.invocations().len(), 1);
    assert!(
        apply(&bound, &incorporate(Vec::new())).is_err(),
        "invocation identity cannot be reused"
    );
    assert_eq!(ledger.order(), &[id(1), id(2)]);
    assert_eq!(ledger.invocations().len(), 0);
}

#[test]
fn malformed_import_and_capacity_fail_closed() {
    let ledger = enqueue(&InputLedger::default(), 1, Vec::new());
    let mut malformed = serde_json::to_value(&ledger).expect("serialize");
    malformed["revisions"][0]["selection"]["revision"] = serde_json::Value::from(0_u64);
    let parsed: InputLedger = serde_json::from_value(malformed).expect("structural import");
    assert_eq!(
        apply(&parsed, &QueueIntent::Withdraw(selected(1, 1))),
        Err(ControlError::InvalidInput)
    );
    let mut full = InputLedger::default();
    for index in 1..=64 {
        full = apply(
            &full,
            &QueueIntent::Enqueue {
                id: id(index),
                text: ControlText::new("x".repeat(8192)).expect("bounded text"),
                dependencies: Vec::new(),
            },
        )
        .expect("within total bytes");
    }
    assert_eq!(
        apply(
            &full,
            &QueueIntent::Enqueue {
                id: id(65),
                text: text("one more byte"),
                dependencies: Vec::new()
            }
        ),
        Err(ControlError::Capacity)
    );
    assert_eq!(full.revisions().len(), 64);
}

#[test]
fn request_capture_excludes_held_dependencies_and_tracks_every_mutation() {
    let ledger = enqueue(&InputLedger::default(), 1, Vec::new());
    let ledger = enqueue(&ledger, 2, vec![id(1)]);
    let ledger = enqueue(&ledger, 3, Vec::new());
    let held = apply(&ledger, &QueueIntent::Hold { selected: selected(1, 1), held: true })
        .expect("hold prerequisite");
    let capture = held.capture().expect("capture");
    assert_eq!(capture.pending(), &[selected(3, 1)]);
    assert_eq!(capture.generation(), 4);
    assert_eq!(capture.conversation(), "User: original input");
    assert_eq!(held.order(), &[id(1), id(2), id(3)], "capture does not consume input");
    let bound = apply(&held, &incorporate(capture.pending().to_vec())).expect("bind");
    let released = apply(&bound, &QueueIntent::Hold { selected: selected(1, 1), held: false })
        .expect("release prerequisite");
    let next = released.capture().expect("capture");
    assert_eq!(next.pending(), &[selected(1, 1), selected(2, 1)]);
    assert_eq!(next.generation(), 5);
    assert_eq!(next.conversation().matches("User: ").count(), 3);
}
