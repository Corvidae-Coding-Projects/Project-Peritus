use super::*;

fn method_fixture(source: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::new();
    fixture.write(
        "crates/app/peritus-product-runner/src/accounting.rs",
        "verus! { pub struct AccountingState; pub struct OtherState; }\n",
    );
    let relative = "crates/app/peritus-product-runner/src/accounting/usage.rs";
    fixture.write(relative, source);
    let path = fixture.path().join(relative);
    (fixture, path)
}

fn accepts(source: &Path, symbol: &str) -> bool {
    let mut diagnostics = Vec::new();
    validate_symbol(
        Path::new("verification/obligations.toml"),
        "OBL-0222",
        "peritus-product-runner",
        Some(source),
        symbol,
        &mut diagnostics,
    );
    diagnostics.is_empty()
}

fn accepts_repository_symbol(owning_crate: &str, source_file: &str, symbol: &str) -> bool {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside the repository");
    let mut diagnostics = Vec::new();
    validate_symbol(
        Path::new("verification/obligations.toml"),
        "OBL-0001",
        owning_crate,
        Some(&repository.join(source_file)),
        symbol,
        &mut diagnostics,
    );
    diagnostics.is_empty()
}

#[test]
fn methods_resolve_the_type_definition_instead_of_the_impl_file() {
    for header in [
        "use super::{OtherState, AccountingState}; impl AccountingState",
        "use super::AccountingState as State; impl State",
        "impl super::AccountingState",
        "impl crate::accounting::AccountingState",
        "use crate::accounting::{self, AccountingState as State}; impl State",
    ] {
        let (_fixture, source) =
            method_fixture(&format!("{header} {{ pub fn apply_usage() {{}} }}"));
        assert!(
            accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"),
            "{header}"
        );
        for wrong in [
            "peritus_product_runner::accounting::usage::AccountingState::apply_usage",
            "peritus_product_runner::accounting::OtherState::apply_usage",
            "peritus_product_runner::accounting::State::apply_usage",
            "peritus_product_runner::accounting::usage::apply_usage",
        ] {
            assert!(!accepts(&source, wrong), "accepted {wrong} from {header}");
        }
    }
}

#[test]
fn protected_base_method_locators_fail_while_the_candidate_owners_resolve() {
    for (owning_crate, source, protected_base, candidate) in [
        (
            "peritus-harness",
            "crates/orchestration/peritus-harness/src/materialization/planner.rs",
            "peritus_harness::materialization::planner::MaterializationPlan::build",
            "peritus_harness::materialization::plan_model::MaterializationPlan::build",
        ),
        (
            "peritus-journal",
            "crates/state/peritus-journal/src/domain/approval_use.rs",
            "peritus_journal::domain::approval_use::SqliteJournal::commit_approval_use",
            "peritus_journal::sqlite::connection::SqliteJournal::commit_approval_use",
        ),
        (
            "peritus-app-protocol",
            "crates/app/peritus-app-protocol/src/subscription/transitions.rs",
            "peritus_app_protocol::subscription::transitions::SubscriptionState::deliver",
            "peritus_app_protocol::subscription::state::SubscriptionState::deliver",
        ),
        (
            "peritus-daemon",
            "crates/app/peritus-daemon/src/startup/runtime/runner.rs",
            "peritus_daemon::startup::runtime::runner::DaemonRuntime::start",
            "peritus_daemon::startup::runtime::DaemonRuntime::start",
        ),
    ] {
        assert!(
            !accepts_repository_symbol(owning_crate, source, protected_base),
            "protected-base locator unexpectedly resolved: {protected_base}"
        );
        assert!(
            accepts_repository_symbol(owning_crate, source, candidate),
            "candidate locator did not resolve: {candidate}"
        );
    }
}

#[test]
fn missing_ambiguous_and_unsupported_type_owners_fail_closed() {
    for header in [
        "impl AccountingState",
        "use super::Missing as AccountingState; impl AccountingState",
        "use super::*; impl AccountingState",
        "use super::AccountingState; use super::OtherState as AccountingState; impl AccountingState",
        "type AccountingState = super::OtherState; impl AccountingState",
    ] {
        let (_fixture, source) = method_fixture(&format!("{header} {{ fn apply_usage() {{}} }}"));
        for owner in [
            "accounting::AccountingState",
            "accounting::OtherState",
            "accounting::usage::AccountingState",
        ] {
            assert!(
                !accepts(&source, &format!("peritus_product_runner::{owner}::apply_usage")),
                "accepted {header} as {owner}"
            );
        }
    }
}

#[test]
fn imports_are_scoped_and_do_not_redirect_other_same_named_methods() {
    let (_fixture, source) = method_fixture(
        "use super::AccountingState; impl AccountingState { fn apply_usage() {} }
         mod nested { struct AccountingState; impl AccountingState { fn apply_usage() {} } }
         fn outer() { use super::OtherState as AccountingState; }
         const TEXT: &str = \"use super::OtherState as AccountingState;\";",
    );
    assert!(accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
    assert!(accepts(
        &source,
        "peritus_product_runner::accounting::usage::nested::AccountingState::apply_usage"
    ));
    assert!(!accepts(&source, "peritus_product_runner::accounting::OtherState::apply_usage"));
}

#[test]
fn alias_spelling_and_import_order_cannot_change_the_defining_owner() {
    let (_fixture, source) = method_fixture(
        "impl AccountingState { fn apply_usage() {} } use super::OtherState as AccountingState;",
    );
    assert!(accepts(&source, "peritus_product_runner::accounting::OtherState::apply_usage"));
    assert!(!accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
}

#[test]
fn ambiguous_definition_files_and_non_item_types_are_rejected() {
    let (fixture, source) =
        method_fixture("use super::AccountingState; impl AccountingState { fn apply_usage() {} }");
    fixture.write(
        "crates/app/peritus-product-runner/src/accounting/mod.rs",
        "pub struct AccountingState;",
    );
    assert!(!accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
    fs::remove_file(fixture.path().join("crates/app/peritus-product-runner/src/accounting/mod.rs"))
        .expect("remove ambiguous file");
    for definition in [
        "fn hidden() { struct AccountingState; }",
        "other_macro! { struct AccountingState; }",
        "// struct AccountingState;\nconst TEXT: &str = \"struct AccountingState;\";",
        "struct AccountingState; struct AccountingState;",
        "type AccountingState = OtherState;",
    ] {
        fixture.write("crates/app/peritus-product-runner/src/accounting.rs", definition);
        assert!(
            !accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"),
            "{definition}"
        );
    }
}

#[test]
fn unresolved_owners_do_not_shift_same_line_declaration_modes() {
    let (_fixture, source) = method_fixture(
        "use super::{OtherState, AccountingState}; verus! { impl Missing { spec fn apply_usage() -> bool { true } } impl OtherState { proof fn apply_usage() {} } impl AccountingState { fn apply_usage() {} } }",
    );
    let text = fs::read_to_string(&source).expect("fixture source");
    let declarations = super::super::manifest_symbol::owned_function_declarations(
        "peritus-product-runner",
        &source,
        &text,
        "apply_usage",
    );
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].path, "peritus_product_runner::accounting::OtherState::apply_usage");
    assert_eq!(
        declarations[1].path,
        "peritus_product_runner::accounting::AccountingState::apply_usage"
    );
    assert_eq!(declarations[0].declaration.mode, Some(crate::api_contract::Mode::Proof));
    assert_eq!(declarations[1].declaration.mode, Some(crate::api_contract::Mode::Exec));
}

#[test]
fn declaration_shaped_macro_or_block_contents_cannot_supply_a_method() {
    for body in [
        "other_macro! { impl AccountingState { fn apply_usage() {} } }",
        "const VALUE: () = { impl AccountingState { fn apply_usage() {} } };",
        "fn hidden() { impl AccountingState { fn apply_usage() {} } }",
    ] {
        let (_fixture, source) = method_fixture(&format!("use super::AccountingState; {body}"));
        assert!(
            !accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"),
            "{body}"
        );
    }
}

#[test]
fn a_file_alone_cannot_supply_an_unbound_relative_owner_path() {
    let (fixture, source) = method_fixture("impl phantom::AccountingState { fn apply_usage() {} }");
    fixture.write(
        "crates/app/peritus-product-runner/src/accounting/usage/phantom.rs",
        "pub struct AccountingState;",
    );
    assert!(!accepts(
        &source,
        "peritus_product_runner::accounting::usage::phantom::AccountingState::apply_usage"
    ));
}

#[test]
fn nested_associated_items_cannot_overwrite_the_enclosing_owner() {
    for decoy in [
        "trait Decoy { impl AccountingState { fn apply_usage() {} } }",
        "impl OtherState { impl AccountingState { fn apply_usage() {} } }",
        "impl OtherState { mod nested { impl super::AccountingState { fn apply_usage() {} } } }",
    ] {
        let (_fixture, source) = method_fixture(&format!(
            "use super::{{OtherState, AccountingState}}; verus! {{ {decoy} }}"
        ));
        assert!(
            !accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"),
            "{decoy}"
        );
    }
}

#[test]
fn explicit_reexports_resolve_to_the_definition_and_cycles_fail_closed() {
    let (fixture, source) =
        method_fixture("use super::AccountingState; impl AccountingState { fn apply_usage() {} }");
    fixture.write(
        "crates/app/peritus-product-runner/src/accounting.rs",
        "mod model; pub use model::State as AccountingState;",
    );
    fixture.write("crates/app/peritus-product-runner/src/accounting/model.rs", "pub struct State;");
    assert!(accepts(&source, "peritus_product_runner::accounting::model::State::apply_usage"));
    assert!(!accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
    fixture.write(
        "crates/app/peritus-product-runner/src/accounting/model.rs",
        "pub use super::AccountingState as State;",
    );
    assert!(!accepts(&source, "peritus_product_runner::accounting::model::State::apply_usage"));
}

#[test]
fn ordinary_trait_impl_locators_keep_their_exact_self_type() {
    let (_fixture, source) = method_fixture(
        "use super::{AccountingState, OtherState}; trait Apply { fn apply_usage(); } impl Apply for AccountingState { fn apply_usage() {} }",
    );
    assert!(accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
    assert!(!accepts(&source, "peritus_product_runner::accounting::OtherState::apply_usage"));
}

#[test]
fn non_item_token_trees_cannot_supply_owners_bindings_or_methods() {
    for decoy in [
        "other_macro!(impl AccountingState { fn apply_usage() {} });",
        "other_macro![impl AccountingState { fn apply_usage() {} }];",
        "#[attribute(impl AccountingState { fn apply_usage() {} })] struct Marker;",
        "macro_rules! decoy { () => { impl AccountingState { fn apply_usage() {} } }; }",
    ] {
        let (_fixture, source) = method_fixture(&format!("use super::AccountingState; {decoy}"));
        assert!(
            !accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"),
            "{decoy}"
        );
    }
    let (_fixture, source) = method_fixture(
        "#[attribute(use super::AccountingState;)] struct Marker; impl AccountingState { fn apply_usage() {} }",
    );
    assert!(!accepts(&source, "peritus_product_runner::accounting::AccountingState::apply_usage"));
}

#[test]
fn skipping_non_items_preserves_the_following_exact_mode_ordinal() {
    for decoy in [
        "other_macro!(impl AccountingState { spec fn apply_usage() -> bool { true } });",
        "other_macro![impl AccountingState { proof fn apply_usage() {} }];",
        "#[attribute(impl AccountingState { proof fn apply_usage() {} })] struct Marker;",
    ] {
        let (_fixture, source) = method_fixture(&format!(
            "use super::AccountingState; verus! {{ {decoy} impl AccountingState {{ fn apply_usage() {{}} }} }}"
        ));
        let text = fs::read_to_string(&source).expect("fixture source");
        let declarations = super::super::manifest_symbol::owned_function_declarations(
            "peritus-product-runner",
            &source,
            &text,
            "apply_usage",
        );
        assert_eq!(declarations.len(), 1, "{decoy}");
        assert_eq!(
            declarations[0].declaration.mode,
            Some(crate::api_contract::Mode::Exec),
            "{decoy}"
        );
    }
}
