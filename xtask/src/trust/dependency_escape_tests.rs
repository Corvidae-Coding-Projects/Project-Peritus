use super::*;

#[test]
fn detects_pinned_dependency_proof_forging_constructors() {
    let source = r"
proof fn forged() {
    builtin::assume_(false);
    let _ghost = Ghost::assume_new();
    let _ghost_fallback = Ghost::<bool>::assume_new_fallback(|| false);
    let _tracked = Tracked::assume_new();
    let _tracked_fallback = Tracked::<bool>::assume_new_fallback(|| false);
}
";
    let constructs: Vec<_> = scan(source).into_iter().map(|item| item.construct).collect();
    for required in [
        Construct::BuiltinAssume,
        Construct::GhostAssumeNew,
        Construct::GhostAssumeNewFallback,
        Construct::TrackedAssumeNew,
        Construct::TrackedAssumeNewFallback,
    ] {
        assert_eq!(constructs.iter().filter(|item| **item == required).count(), 1);
        assert!(Construct::is_known_label(required.label()));
    }
}

#[test]
fn trusted_operation_imports_reexports_and_aliases_are_prohibited() {
    let source = r"
use vstd::pervasive::assume as accept;
use vstd::prelude::Ghost as Phantom;
use vstd::pervasive::*;
pub use vstd::prelude::*;
type Hidden<T> = Tracked<T>;
";
    assert_eq!(
        scan(source)
            .iter()
            .filter(|item| item.construct == Construct::ProhibitedTrustedImport)
            .count(),
        5
    );
    let fixture = TestDirectory::new();
    write_source(&fixture, "fixture/src/lib.rs", source);
    let error = check(fixture.path(), &policy(vec![PathBuf::from("fixture")]))
        .expect_err("trusted imports must fail even inside the trusted root");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|item| item.message().contains("imports, reexports, or aliases"))
    );
}

#[test]
fn aliased_assume_new_call_is_still_counted() {
    let source = r"
type Forge<T> = Tracked<T>;
proof fn forge() { let _value = Forge::<bool>::assume_new(); }
";
    let constructs: Vec<_> = scan(source).into_iter().map(|item| item.construct).collect();
    assert!(constructs.contains(&Construct::ProhibitedTrustedImport));
    assert!(constructs.contains(&Construct::AssumeNew));
}

#[test]
fn nested_trusted_items_are_detected_but_prohibited_in_the_tcb() {
    let source = r"
mod nested {
    proof fn duplicate() { assume(false); }
}
proof fn duplicate() { assume(false); }
";
    let occurrences = scan(source);
    let assumptions: Vec<_> =
        occurrences.iter().filter(|item| item.construct == Construct::Assume).collect();
    assert_eq!(assumptions.len(), 2);
    assert!(assumptions[0].nested_item_scope);
    assert!(!assumptions[1].nested_item_scope);
    let fixture = TestDirectory::new();
    write_source(&fixture, "fixture/src/lib.rs", source);
    let error = check(fixture.path(), &policy(vec![PathBuf::from("fixture")]))
        .expect_err("nested trusted items must be prohibited in the trusted root");
    assert!(error.diagnostics().iter().any(|item| item.message().contains("nested")));
}
