use super::{Command, discover_workspace_root, parse};
use crate::error::ErrorCode;
use crate::product_package::qualification::QualificationInput;
use std::ffi::OsString;
use std::fs;

#[test]
fn empty_arguments_show_help() {
    assert_eq!(parse(Vec::<OsString>::new()).expect("empty args are valid"), Command::Help);
}

#[test]
fn unknown_command_has_stable_typed_error() {
    let error = parse([OsString::from("unknown")]).expect_err("unknown command must fail");
    assert_eq!(error.code(), ErrorCode::Invocation);
    assert!(error.render().contains("cargo xtask help"));
}

#[test]
fn native_qualification_commands_are_first_class_and_reject_extra_arguments() {
    for (name, expected) in [
        ("product-native-qualification", Command::ProductNativeQualification),
        ("product-native-qualification-prepare", Command::ProductNativeQualificationPrepare),
        ("product-native-qualification-restore", Command::ProductNativeQualificationRestore),
    ] {
        assert_eq!(parse([OsString::from(name)]).expect("qualification command"), expected);
        assert!(parse([OsString::from(name), OsString::from("extra")]).is_err());
    }
}

#[test]
fn lifecycle_commands_explicitly_select_build_or_prepared_inputs() {
    for (name, input) in [
        ("release-bootstrap-smoke", QualificationInput::Build),
        ("release-bootstrap-prepared-smoke", QualificationInput::Prepared),
    ] {
        assert_eq!(
            parse([OsString::from(name)]).expect("lifecycle command"),
            Command::ReleaseBootstrapSmoke { input }
        );
        assert!(parse([OsString::from(name), OsString::from("extra")]).is_err());
    }
}

#[test]
fn workspace_root_is_discovered_from_the_xtask_directory() {
    let crate_root = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
        .expect("xtask manifest directory must be canonicalizable");
    let workspace = discover_workspace_root(&crate_root)
        .expect("xtask must be nested under the Peritus workspace root");
    assert_eq!(
        workspace.join("xtask").canonicalize().expect("discovered xtask must canonicalize"),
        crate_root
    );
    assert!(workspace.join("architecture.toml").is_file());
}

#[cfg(windows)]
#[test]
fn discovered_workspace_root_is_safe_to_pass_back_to_child_processes() {
    use std::path::{Component, Prefix};

    let crate_root = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
        .expect("xtask manifest directory must be canonicalizable");
    let workspace = discover_workspace_root(&crate_root)
        .expect("xtask must be nested under the Peritus workspace root");
    assert!(!matches!(
        workspace.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _))
    ));
}
