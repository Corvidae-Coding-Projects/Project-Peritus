//! Script the real setup terminal, then publish and reopen immutable product state.
use super::*;
use peritus_product_state::{InstallIdentity, ProductState, ProviderKind, ReviewerSelection};
use std::io::Cursor;

fn configured() -> ProviderSelection {
    ProviderSelection::new(vec![ProviderKind::CodexAccount], Some(ProviderKind::CodexAccount))
        .unwrap()
        .with_account_models([(ProviderKind::CodexAccount, "exact-writer-model".into())].into())
        .unwrap()
}

fn replay(
    input: &str,
    current: &ProviderSelection,
) -> Result<(ProviderSelection, String), LauncherError> {
    let mut output = Vec::new();
    let result = select(
        &mut Terminal::from_streams(Cursor::new(input), &mut output),
        current.clone(),
        current,
        true,
    )?;
    Ok((result, String::from_utf8(output).unwrap()))
}

#[test]
fn startup_collects_every_role_and_restart_preserves_exact_choices() {
    // Keep the saved writer ID, explicitly select XHigh, explicitly copy it to both roles.
    let (selection, output) = replay("\n\nxhigh\n0\n0\n", &configured()).unwrap();
    for name in ["Writer", "Reviewer", "Fixer"] {
        assert!(output.contains(&format!("{name} provider")));
    }
    assert!(output.contains("/models"));
    let dir = tempfile::tempdir().unwrap();
    let store = crate::persistence::ProductStateStore::open(dir.path().to_owned()).unwrap();
    let mut state = ProductState::new(InstallIdentity::new([1; 16], [2; 16]).unwrap());
    state.configure_providers(selection.clone());
    store.commit(&state).unwrap();
    drop(store);
    let reopened = crate::persistence::ProductStateStore::open(dir.path().to_owned())
        .unwrap()
        .load_or_initialize()
        .unwrap();
    assert_eq!(reopened.providers(), &selection);
    let roles = reopened.providers().role_models().unwrap();
    for choice in [roles.writer(), roles.reviewer(), roles.fixer()] {
        assert_eq!(choice.model(), "exact-writer-model");
        assert_eq!(choice.effort(), RoleEffort::XHigh);
    }
    // A completed setup can retain all choices on settings replay; no effort is reset.
    let (retained, _) = replay("\n\n\n\n\n\n\n\n\n", &selection).unwrap();
    assert_eq!(retained, selection);
}

#[test]
fn legacy_reviewer_is_retained_but_effort_requires_an_explicit_choice() {
    let current = configured()
        .with_reviewer(Some(
            ReviewerSelection::new(ProviderKind::CodexAccount, "legacy-review-model".into(), true)
                .unwrap(),
        ))
        .unwrap();
    let (selection, output) =
        replay("\n\nxhigh\n\n\n\nhigh\n1\nmanual exact-fixer-model\nmedium\n", &current).unwrap();
    let roles = selection.role_models().unwrap();
    assert_eq!(roles.reviewer().model(), "legacy-review-model");
    assert_eq!(roles.reviewer().effort(), RoleEffort::High);
    assert_eq!(roles.fixer().model(), "exact-fixer-model");
    assert_eq!(roles.fixer().effort(), RoleEffort::Medium);
    assert!(output.contains("Select an explicit displayed effort."));
}

#[test]
fn incomplete_or_cancelled_setup_never_publishes_partial_roles() {
    let current = configured();
    for input in ["", "\n\nxhigh\n", "\n\nxhigh\n0\nq\n"] {
        assert!(replay(input, &current).is_err());
        assert!(current.role_models().is_none());
    }
    assert!(
        RoleModelSelection::new(ProviderKind::CodexAccount, "id".into(), true, RoleEffort::Off)
            .is_err()
    );
    let disabled =
        RoleModelSelection::new(ProviderKind::ClaudeAccount, "id".into(), true, RoleEffort::High)
            .unwrap();
    assert!(
        current
            .clone()
            .with_role_models(Some(RoleModelDefaults::new(
                disabled.clone(),
                disabled.clone(),
                disabled
            )))
            .is_err()
    );
    let mut malformed =
        serde_json::to_value(ProductState::new(InstallIdentity::new([1; 16], [2; 16]).unwrap()))
            .unwrap();
    malformed["providers"] =
        serde_json::to_value(replay("\n\nxhigh\n0\n0\n", &current).unwrap().0).unwrap();
    malformed["providers"]["role_models"]["fixer"]["model"] = "bad model".into();
    assert!(ProductState::parse_json(&serde_json::to_vec(&malformed).unwrap()).is_err());
}
