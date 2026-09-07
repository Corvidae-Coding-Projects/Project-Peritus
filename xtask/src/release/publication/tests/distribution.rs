use super::*;

#[test]
fn distribution_matrix_builds_and_signs_each_format_on_each_native_architecture() {
    let document = workflow(".github/workflows/release.yml");
    for job in ["distro-image", "distro-compile", "distro-build", "distro-sign"] {
        let definition = &document["jobs"][job];
        assert_eq!(definition["timeout-minutes"].as_i64(), Some(10));
        assert_eq!(definition["strategy"]["fail-fast"].as_bool(), Some(false));
        let rows = definition["strategy"]["matrix"]["include"].as_vec().expect("distro matrix");
        assert_eq!(rows.len(), 4);
        for (runner, architecture) in [("ubuntu-24.04", "x86_64"), ("ubuntu-24.04-arm", "aarch64")]
        {
            for format in ["deb", "rpm"] {
                assert_eq!(
                    rows.iter()
                        .filter(|row| row["os"].as_str() == Some(runner)
                            && row["arch"].as_str() == Some(architecture)
                            && row["format"].as_str() == Some(format))
                        .count(),
                    1,
                    "{job}/{format}/{architecture} must have exactly one native runner"
                );
            }
        }
    }
    assert_eq!(document["jobs"]["distro-compile"]["needs"].as_str(), Some("distro-image"));
    assert_eq!(document["jobs"]["distro-build"]["needs"].as_str(), Some("distro-compile"));
    let signing = &document["jobs"]["distro-sign"];
    assert_eq!(signing["environment"].as_str(), Some("release-signing"));
    let signing_needs = signing["needs"].as_vec().expect("signing dependencies");
    assert!(signing_needs.iter().any(|value| value.as_str() == Some("distro-build")));
    let staging_needs =
        document["jobs"]["stage-draft"]["needs"].as_vec().expect("staging dependencies");
    assert!(staging_needs.iter().any(|value| value.as_str() == Some("distro-sign")));
    let commands = signing["steps"]
        .as_vec()
        .expect("signing steps")
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        [
            "cargo run --locked --package xtask -- distro-image-restore",
            "cargo run --locked --package xtask -- distro-sign-ci",
            "cargo run --locked --package xtask -- distro-upload",
        ]
    );
}

#[test]
fn release_build_capacity_is_scoped_without_loosening_profiles_or_deadlines() {
    let document = workflow(".github/workflows/release.yml");
    assert_eq!(document["env"]["CARGO_BUILD_JOBS"].as_str(), Some("2"));
    let jobs = &document["jobs"];
    assert_eq!(
        jobs["build-binary"]["env"]["CARGO_BUILD_JOBS"].as_str(),
        Some("${{ matrix.target.os == 'macos-15-intel' && '4' || '2' }}")
    );
    assert_eq!(jobs["distro-build"]["env"]["PERITUS_PACKAGE_BUILD_JOBS"].as_str(), Some("4"));
    assert_eq!(jobs["distro-compile"]["env"]["PERITUS_PACKAGE_BUILD_JOBS"].as_str(), Some("4"));
    for (name, job) in jobs.as_hash().expect("release jobs") {
        assert_eq!(job["timeout-minutes"].as_i64(), Some(10));
        if !matches!(name.as_str(), Some("distro-build" | "distro-compile")) {
            assert!(job["env"]["PERITUS_PACKAGE_BUILD_JOBS"].is_badvalue());
        }
    }
    let commands = jobs["build-binary"]["steps"]
        .as_vec()
        .expect("binary build steps")
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0],
        "cargo build --release --locked --package ${{ matrix.target.package }} --bin ${{ matrix.target.binary }}"
    );
}

#[test]
fn distribution_downloads_cannot_cross_runs_formats_or_architectures() {
    let document = workflow(".github/workflows/release.yml");
    for (job, count) in [("distro-compile", 1), ("distro-build", 2), ("distro-sign", 2)] {
        let steps = document["jobs"][job]["steps"].as_vec().expect("distribution steps");
        let downloads = steps
            .iter()
            .filter(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("actions/download-artifact@"))
            })
            .collect::<Vec<_>>();
        assert_eq!(downloads.len(), count);
        for download in downloads {
            let options = &download["with"];
            assert_eq!(options.as_hash().expect("same-run options").len(), 2);
            assert!(matches!(
                options["name"].as_str(),
                Some(
                    "distro-image-${{ matrix.format }}-${{ matrix.arch }}"
                        | "distro-compiled-${{ matrix.format }}-${{ matrix.arch }}"
                        | "distro-unsigned-${{ matrix.format }}-${{ matrix.arch }}"
                )
            ));
            assert!(matches!(
                options["path"].as_str(),
                Some("target" | "target/distro-compiled" | "dist/packages/${{ matrix.format }}")
            ));
        }
    }
}

#[test]
fn distribution_staging_runs_exact_native_commands_and_retains_the_complete_tree() {
    let document = workflow(".github/workflows/release.yml");
    for (job, operation) in
        [("distro-compile", "distro-compile"), ("distro-build", "distro-package-compiled")]
    {
        let steps = document["jobs"][job]["steps"].as_vec().expect("staged distribution steps");
        let commands = steps.iter().filter_map(|step| step["run"].as_str()).collect::<Vec<_>>();
        assert_eq!(
            commands,
            [
                "cargo run --locked --package xtask -- distro-image-restore".to_owned(),
                format!("cargo run --locked --package xtask -- {operation}"),
            ]
        );
    }
    let upload = document["jobs"]["distro-compile"]["steps"]
        .as_vec()
        .expect("compile steps")
        .iter()
        .find(|step| {
            step["uses"].as_str().is_some_and(|value| value.starts_with("actions/upload-artifact@"))
        })
        .expect("compiled tree upload");
    assert_eq!(
        upload["with"]["name"].as_str(),
        Some("distro-compiled-${{ matrix.format }}-${{ matrix.arch }}")
    );
    assert_eq!(upload["with"]["path"].as_str(), Some("target/distro-compiled"));
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
    assert_eq!(upload["with"]["compression-level"].as_i64(), Some(0));
}
