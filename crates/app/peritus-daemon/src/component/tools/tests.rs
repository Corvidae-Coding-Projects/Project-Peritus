use peritus_tool_router::RouterLimits;

use super::{ToolComponentErrorKind, ToolComponents, ToolDispatcherRoute};

fn limits() -> RouterLimits {
    RouterLimits::new(8, 32).expect("valid test limits")
}

#[test]
fn configured_allowlist_is_canonical_and_exact() {
    let allowed =
        vec!["shell.exec".to_owned(), "fs.read".to_owned(), "quality.discover".to_owned()];
    let tools = ToolComponents::build(&allowed, limits()).expect("configured tools");
    let names: Vec<_> = tools
        .registrations()
        .iter()
        .map(|registration| registration.descriptor().name().as_str())
        .collect();
    assert_eq!(names, ["fs.read", "quality.discover", "shell.exec"]);
    assert_eq!(tools.operations().as_slice().len(), 3);
    assert!(tools.registry().is_some());
}

#[test]
fn empty_allowlist_exposes_no_router_or_operations() {
    let tools = ToolComponents::build(&[], limits()).expect("empty tools");
    assert!(tools.is_empty());
    assert!(tools.registry().is_none());
    assert!(tools.operations().as_slice().is_empty());
}

#[test]
fn duplicates_and_unknown_routes_are_rejected() {
    let duplicate = vec!["fs.read".to_owned(), "fs.read".to_owned()];
    let error = ToolComponents::build(&duplicate, limits()).expect_err("duplicate rejected");
    assert_eq!(error.kind(), ToolComponentErrorKind::DuplicateTool);

    let unknown = vec!["fs.teleport".to_owned()];
    let error = ToolComponents::build(&unknown, limits()).expect_err("unknown rejected");
    assert_eq!(error.kind(), ToolComponentErrorKind::UnknownTool);
}

#[test]
fn unsupported_git_merge_is_never_a_production_handler() {
    let allowed = vec!["git.merge".to_owned()];
    let error = ToolComponents::build(&allowed, limits()).expect_err("merge rejected");
    assert_eq!(error.kind(), ToolComponentErrorKind::UnknownTool);
}

#[test]
fn every_registration_has_the_declared_route_name() {
    let allowed = vec![
        "fs.patch".to_owned(),
        "git.status".to_owned(),
        "quality.run".to_owned(),
        "shell.script".to_owned(),
    ];
    let tools = ToolComponents::build(&allowed, limits()).expect("configured tools");
    for registration in tools.registrations() {
        assert_eq!(registration.descriptor().name().as_str(), registration.route().name());
    }
    assert_eq!(tools.registrations()[2].route(), ToolDispatcherRoute::QualityRun);
}
