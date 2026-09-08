//! The native library handoff must preserve each consumer and independent compilation.

use super::*;

const DAEMON: &str = "${{ (matrix.target.os == 'macos-15-intel' || matrix.target.os == 'windows-2025') && matrix.target.binary == 'peritusd' }}";
const CONSUMERS: &str = "${{ (matrix.target.os == 'macos-15-intel' && (matrix.target.binary == 'peritusd' || matrix.target.binary == 'peritus')) || (matrix.target.os == 'windows-2025' && matrix.target.binary == 'peritusd') }}";

#[test]
fn previous_staging_comparison_is_manual_only_and_cannot_supply_release_products() {
    let document = workflow(".github/workflows/release.yml");
    let job = &document["jobs"]["check-native-staging"];
    assert_eq!(job["if"].as_str(), Some("${{ github.event_name == 'workflow_dispatch' }}"));
    assert_eq!(job["needs"].as_str(), Some("build-binary"));
    assert_eq!(job["timeout-minutes"].as_i64(), Some(10));
    assert_eq!(job["env"]["PERITUS_RELEASE_BUILD_ROLE"].as_str(), Some("primary"));
    let rows = job["strategy"]["matrix"]["target"].as_vec().expect("diagnostic targets");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["os"].as_str(), Some("macos-15-intel"));
    assert_eq!(rows[0]["binary"].as_str(), Some("peritus"));
    assert_eq!(rows[1]["os"].as_str(), Some("windows-2025"));
    assert_eq!(rows[1]["binary"].as_str(), Some("peritusd"));
    let steps = job["steps"].as_vec().expect("diagnostic steps");
    let uploads = steps
        .iter()
        .filter(|step| step["uses"].as_str().is_some_and(|name| name.contains("upload-artifact")))
        .collect::<Vec<_>>();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0]["with"]["path"].as_str(), Some("target/native-staging-check.json"));
    assert_eq!(
        uploads[0]["with"]["name"].as_str(),
        Some("release-staging-check-${{ matrix.target.os }}")
    );
    assert_eq!(document["jobs"]["assemble"]["needs"].as_str(), Some("build-binary"));
}

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
fn intel_cli_library_stage_retains_the_original_role_bound_daemon_chain_without_a_binary() {
    let document = workflow(".github/workflows/release.yml");
    let stage = &document["jobs"]["build-cli-library"];
    assert_eq!(stage["needs"].as_str(), Some("build-daemon-library"));
    assert_eq!(stage["runs-on"].as_str(), Some("macos-15-intel"));
    assert_eq!(stage["timeout-minutes"].as_i64(), Some(10));
    assert_eq!(stage["strategy"]["fail-fast"].as_bool(), Some(false));
    assert_eq!(stage["strategy"]["matrix"].as_hash().expect("CLI matrix").len(), 1);
    assert_eq!(
        stage["strategy"]["matrix"]["build"].as_vec().expect("CLI roles"),
        &[Yaml::String("primary".into()), Yaml::String("independent".into())]
    );
    assert_eq!(stage["env"]["CARGO_BUILD_JOBS"].as_str(), Some("4"));
    assert_eq!(stage["env"]["PERITUS_RELEASE_BUILD_ROLE"].as_str(), Some("${{ matrix.build }}"));
    let steps = stage["steps"].as_vec().expect("CLI steps");
    assert_eq!(
        steps.iter().filter_map(|step| step["run"].as_str()).collect::<Vec<_>>(),
        ["cargo run --locked --package xtask -- release-cli-library"]
    );
    assert!(
        !steps.iter().any(|step| step["uses"].as_str().is_some_and(|name| name.contains("cache")))
    );
    let downloads = steps
        .iter()
        .filter(|step| step["uses"].as_str().is_some_and(|name| name.contains("download-artifact")))
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0]["with"].as_hash().expect("same-run options").len(), 2);
    assert_eq!(
        downloads[0]["with"]["name"].as_str(),
        Some("release-libraries-${{ matrix.build }}-macos-15-intel-peritusd")
    );
    assert_eq!(downloads[0]["with"]["path"].as_str(), Some("target/native-daemon-libraries"));
    let upload = steps.last().expect("CLI library upload");
    assert_eq!(
        upload["with"]["name"].as_str(),
        Some("release-libraries-${{ matrix.build }}-macos-15-intel-peritus")
    );
    assert_eq!(upload["with"]["path"].as_str(), Some("target/native-cli-libraries"));
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
}

#[test]
fn native_library_producers_are_independent_fresh_and_scoped_to_oversized_native_targets() {
    let document = workflow(".github/workflows/release.yml");
    let producer = &document["jobs"]["build-daemon-library"];
    assert_eq!(producer["runs-on"].as_str(), Some("${{ matrix.os }}"));
    assert_eq!(producer["timeout-minutes"].as_i64(), Some(10));
    assert_eq!(producer["strategy"]["fail-fast"].as_bool(), Some(false));
    let matrix = &producer["strategy"]["matrix"];
    assert_eq!(matrix.as_hash().expect("library matrix").len(), 2);
    assert_eq!(
        matrix["os"].as_vec().expect("native hosts"),
        &[Yaml::String("macos-15-intel".into()), Yaml::String("windows-2025".into())]
    );
    assert_eq!(
        matrix["build"].as_vec().expect("roles"),
        &[Yaml::String("primary".into()), Yaml::String("independent".into())]
    );
    assert_eq!(
        producer["env"]["CARGO_BUILD_JOBS"].as_str(),
        Some("${{ matrix.os == 'macos-15-intel' && '4' || '2' }}")
    );
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
        Some("release-libraries-${{ matrix.build }}-${{ matrix.os }}-peritusd")
    );
    assert_eq!(upload["with"]["path"].as_str(), Some("target/native-daemon-libraries"));
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
    assert_eq!(
        document["jobs"]["build-binary"]["needs"].as_vec().expect("producer dependencies"),
        &[Yaml::String("build-daemon-library".into()), Yaml::String("build-cli-library".into())]
    );
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
    assert_eq!(
        commands[1]["if"].as_str(),
        Some("${{ matrix.target.os == 'windows-2025' && matrix.target.binary != 'peritusd' }}")
    );
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
    assert_eq!(
        download["if"].as_str(),
        Some("${{ matrix.os == 'macos-15-intel' || matrix.os == 'windows-2025' }}")
    );
    assert_eq!(download["with"].as_hash().expect("same-run evidence").len(), 3);
    assert_eq!(
        download["with"]["pattern"].as_str(),
        Some("release-compile-${{ matrix.build }}-${{ matrix.os }}--*")
    );
    assert_eq!(download["with"]["merge-multiple"].as_bool(), Some(true));
}
