//! The native library handoff must preserve each consumer and independent compilation.

use super::*;

const DAEMON: &str =
    "${{ matrix.target.os == 'macos-15-intel' && matrix.target.binary == 'peritusd' }}";
const CONSUMERS: &str = "${{ matrix.target.os == 'macos-15-intel' && (matrix.target.binary == 'peritusd' || matrix.target.binary == 'peritus') }}";

#[test]
fn intel_cli_has_a_reviewed_same_role_library_consumer() {
    let document = workflow(".github/workflows/release.yml");
    let steps = document["jobs"]["build-binary"]["steps"].as_vec().expect("binary steps");
    let consumer = steps
        .iter()
        .find(|step| {
            step["run"].as_str() == Some("cargo run --locked --package xtask -- release-cli-binary")
        })
        .expect("CLI must consume the verified libraries instead of a cold oversized build");
    assert_eq!(
        consumer["if"].as_str(),
        Some("${{ matrix.target.os == 'macos-15-intel' && matrix.target.binary == 'peritus' }}")
    );
    assert_eq!(consumer["env"]["PERITUS_RELEASE_BUILD_ROLE"].as_str(), Some("${{ matrix.build }}"));
}

#[test]
fn native_library_producers_are_independent_fresh_and_scoped_to_intel_macos() {
    let document = workflow(".github/workflows/release.yml");
    let producer = &document["jobs"]["build-daemon-library"];
    assert_eq!(producer["runs-on"].as_str(), Some("macos-15-intel"));
    assert_eq!(producer["timeout-minutes"].as_i64(), Some(10));
    assert_eq!(producer["strategy"]["fail-fast"].as_bool(), Some(false));
    let matrix = &producer["strategy"]["matrix"];
    assert_eq!(matrix.as_hash().expect("library matrix").len(), 1);
    assert_eq!(
        matrix["build"].as_vec().expect("roles"),
        &[Yaml::String("primary".into()), Yaml::String("independent".into())]
    );
    assert_eq!(producer["env"]["CARGO_BUILD_JOBS"].as_str(), Some("4"));
    assert_eq!(producer["env"]["PERITUS_RELEASE_BUILD_ROLE"].as_str(), Some("${{ matrix.build }}"));
    let steps = producer["steps"].as_vec().expect("library steps");
    assert!(!steps.iter().any(|step| {
        step["uses"]
            .as_str()
            .is_some_and(|name| name.contains("cache") || name.contains("download-artifact"))
    }));
    assert_eq!(
        steps.iter().filter_map(|step| step["run"].as_str()).collect::<Vec<_>>(),
        ["cargo run --locked --package xtask -- release-daemon-library"]
    );
    let upload = steps.last().expect("library upload");
    assert_eq!(
        upload["with"]["name"].as_str(),
        Some("release-libraries-${{ matrix.build }}-macos-15-intel-peritusd")
    );
    assert_eq!(upload["with"]["path"].as_str(), Some("target/native-daemon-libraries"));
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
    assert_eq!(document["jobs"]["build-binary"]["needs"].as_str(), Some("build-daemon-library"));
}

#[test]
fn every_binary_keeps_its_native_command_or_uses_only_its_verified_daemon_library() {
    let document = workflow(".github/workflows/release.yml");
    let steps = document["jobs"]["build-binary"]["steps"].as_vec().expect("binary steps");
    let commands = steps.iter().filter(|step| step["run"].as_str().is_some()).collect::<Vec<_>>();
    assert_eq!(commands.len(), 5);
    assert_eq!(
        commands[0]["if"].as_str(),
        Some(
            "${{ runner.os != 'Windows' && (matrix.target.os != 'macos-15-intel' || (matrix.target.binary != 'peritusd' && matrix.target.binary != 'peritus')) }}"
        )
    );
    assert_eq!(commands[1]["if"].as_str(), Some("${{ matrix.target.os == 'windows-2025' }}"));
    assert_eq!(commands[2]["if"].as_str(), Some("${{ matrix.target.os == 'windows-11-arm' }}"));
    assert_eq!(commands[3]["if"].as_str(), Some(DAEMON));
    assert_eq!(
        commands[3]["env"]["PERITUS_RELEASE_BUILD_ROLE"].as_str(),
        Some("${{ matrix.build }}")
    );
    let evidence = steps
        .iter()
        .find(|step| {
            step["with"]["path"].as_str()
                == Some("target/native-${{ matrix.target.binary }}-build.json")
        })
        .expect("producer evidence");
    assert_eq!(evidence["if"].as_str(), Some(CONSUMERS));
    assert_eq!(
        evidence["with"]["name"].as_str(),
        Some(
            "release-compile-${{ matrix.build }}-${{ matrix.target.os }}--${{ matrix.target.binary }}"
        )
    );
    assert_eq!(evidence["with"]["if-no-files-found"].as_str(), Some("error"));
    let assembly = document["jobs"]["assemble"]["steps"].as_vec().expect("assembly steps");
    let download = assembly
        .iter()
        .find(|step| step["with"]["path"].as_str() == Some("target/native-compile-record"))
        .expect("assembly evidence");
    assert_eq!(download["if"].as_str(), Some("${{ matrix.os == 'macos-15-intel' }}"));
    assert_eq!(download["with"].as_hash().expect("same-run evidence").len(), 3);
    assert_eq!(
        download["with"]["pattern"].as_str(),
        Some("release-compile-${{ matrix.build }}-${{ matrix.os }}--*")
    );
    assert_eq!(download["with"]["merge-multiple"].as_bool(), Some(true));
}
