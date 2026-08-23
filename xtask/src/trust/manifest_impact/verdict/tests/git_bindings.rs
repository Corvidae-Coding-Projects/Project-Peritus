use super::*;

#[test]
fn nonexistent_and_wrong_kind_objects_fail_closed() {
    let fixture = GitFixture::new();
    let change = fixture_change();

    let mut nonexistent = verdict(&fixture, &change);
    nonexistent.implementation_commit = repeated('f', 40);
    assert!(
        diagnostics(&fixture, &change, &nonexistent)
            .iter()
            .any(|item| item.message().contains("not an available commit"))
    );

    let mut wrong_object = verdict(&fixture, &change);
    wrong_object.implementation_commit = fixture.blob();
    assert!(
        diagnostics(&fixture, &change, &wrong_object)
            .iter()
            .any(|item| item.message().contains("not an available commit"))
    );
}

#[test]
fn wrong_tree_and_unrelated_implementation_fail_closed() {
    let fixture = GitFixture::new();
    let change = fixture_change();

    let mut wrong_tree = verdict(&fixture, &change);
    let base_tree_expression = format!("{}^{{tree}}", fixture.authorization);
    wrong_tree.implementation_tree = stdout(&fixture.root, &["rev-parse", &base_tree_expression]);
    assert!(
        diagnostics(&fixture, &change, &wrong_tree)
            .iter()
            .any(|item| item.message().contains("not the exact tree"))
    );

    let mut unrelated = verdict(&fixture, &change);
    unrelated.implementation_commit = fixture.unrelated_commit();
    assert!(
        diagnostics(&fixture, &change, &unrelated)
            .iter()
            .any(|item| item.message().contains("does not descend"))
    );
}

#[test]
fn implementation_tree_snapshot_substitution_fails_closed() {
    let fixture = GitFixture::new();
    let mut substituted_change = fixture_change();
    substituted_change.source_changes[0].current.as_mut().expect("current snapshot").sha256 =
        repeated('a', 64);
    let substituted_verdict = verdict(&fixture, &substituted_change);
    assert!(
        diagnostics(&fixture, &substituted_change, &substituted_verdict)
            .iter()
            .any(|item| item.message().contains("does not match the implementation tree"))
    );
}
