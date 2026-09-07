//! Independent native builds must not share product binaries or skip comparison.

use super::*;

#[test]
fn manual_candidate_validation_cannot_create_sign_or_publish_a_release() {
    let document = workflow(".github/workflows/release.yml");
    assert!(
        document["on"]
            .as_hash()
            .expect("triggers")
            .contains_key(&Yaml::String("workflow_dispatch".into()))
    );
    assert_eq!(
        document["jobs"]["create-draft"]["if"].as_str(),
        Some("${{ github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v') }}")
    );
    for job in ["attest", "distro-sign"] {
        assert!(
            document["jobs"][job]["needs"]
                .as_vec()
                .expect("draft-dependent effects")
                .iter()
                .any(|value| value.as_str() == Some("create-draft"))
        );
        assert!(
            document["jobs"][job]["if"].is_badvalue(),
            "must not override skipped draft dependency"
        );
    }
}

#[test]
fn independent_build_is_a_real_cartesian_axis_for_every_native_binary_and_archive() {
    let document = workflow(".github/workflows/release.yml");
    for (job, other_axis, count) in [("build-binary", "target", 24), ("assemble", "os", 6)] {
        let matrix = &document["jobs"][job]["strategy"]["matrix"];
        let axes = matrix.as_hash().expect("matrix");
        assert_eq!(axes.len(), 2, "no include rows that collapse the independent-build axis");
        assert_eq!(matrix[other_axis].as_vec().expect("native inputs").len(), count);
        assert_eq!(
            matrix["build"].as_vec().expect("independent build axis"),
            &[Yaml::String("primary".into()), Yaml::String("independent".into()),]
        );
        assert_eq!(document["jobs"][job]["timeout-minutes"].as_i64(), Some(10));
        let steps = document["jobs"][job]["steps"].as_vec().expect("steps");
        assert!(
            !steps
                .iter()
                .any(|step| step["uses"].as_str().is_some_and(|value| value.contains("cache")))
        );
    }
    let build = document["jobs"]["build-binary"]["steps"].as_vec().expect("binary steps");
    let downloads = build
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|value| value.starts_with("actions/download-artifact@"))
        })
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 1, "only the role-bound native library handoff is allowed");
    let download = downloads[0];
    assert_eq!(
        download["if"].as_str(),
        Some(
            "${{ matrix.target.os == 'macos-15-intel' && (matrix.target.binary == 'peritusd' || matrix.target.binary == 'peritus') }}"
        )
    );
    assert_eq!(download["with"].as_hash().expect("same-run library inputs").len(), 2);
    assert_eq!(
        download["with"]["name"].as_str(),
        Some("release-libraries-${{ matrix.build }}-${{ matrix.target.os }}-peritusd")
    );
    assert_eq!(download["with"]["path"].as_str(), Some("target/native-daemon-libraries"));
    let upload = build.last().expect("binary upload");
    assert_eq!(
        upload["with"]["name"].as_str(),
        Some(
            "release-bin-${{ matrix.build }}-${{ matrix.target.os }}--${{ matrix.target.binary }}"
        )
    );
    let assemble = document["jobs"]["assemble"]["steps"].as_vec().expect("assembly steps");
    let download = assemble
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|value| value.starts_with("actions/download-artifact@"))
        })
        .expect("binaries");
    assert_eq!(
        download["with"]["pattern"].as_str(),
        Some("release-bin-${{ matrix.build }}-${{ matrix.os }}--*")
    );
    assert_eq!(download["with"].as_hash().expect("same-run options").len(), 3);
    assert_eq!(
        assemble.last().expect("package upload")["with"]["name"].as_str(),
        Some("release-package-${{ matrix.build }}-${{ matrix.os }}")
    );
}

#[test]
fn comparison_is_required_before_attestation_and_uses_both_exact_same_run_outputs() {
    let document = workflow(".github/workflows/release.yml");
    let comparison = &document["jobs"]["compare-native"];
    assert_eq!(comparison["needs"].as_str(), Some("assemble"));
    let steps = comparison["steps"].as_vec().expect("comparison steps");
    let downloads = steps
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|value| value.starts_with("actions/download-artifact@"))
        })
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 2);
    for (download, role) in downloads.iter().zip(["primary", "independent"]) {
        assert_eq!(
            download["with"]["name"].as_str(),
            Some(format!("release-package-{role}-${{{{ matrix.os }}}}").as_str())
        );
        assert_eq!(
            download["with"]["path"].as_str(),
            Some(format!("target/native-rebuild/{role}").as_str())
        );
        assert_eq!(download["with"].as_hash().expect("same-run options").len(), 2);
    }
    let commands = steps.iter().filter_map(|step| step["run"].as_str()).collect::<Vec<_>>();
    assert_eq!(commands, ["cargo run --locked --package xtask -- release-rebuild-compare"]);
    assert_eq!(steps.last().expect("retain comparison")["if"].as_str(), Some("${{ always() }}"));
    let attestation_needs =
        document["jobs"]["attest"]["needs"].as_vec().expect("attestation dependencies");
    assert_eq!(
        attestation_needs,
        &[Yaml::String("create-draft".into()), Yaml::String("compare-native".into())]
    );
}
