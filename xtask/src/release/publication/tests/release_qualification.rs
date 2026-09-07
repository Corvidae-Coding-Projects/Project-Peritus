//! Release qualification is wired to exact staged archives on all native targets.

use super::*;

fn steps<'a>(document: &'a Yaml, job: &str) -> &'a Vec<Yaml> {
    document["jobs"][job]["steps"].as_vec().expect("job steps")
}

fn commands<'a>(document: &'a Yaml, job: &str) -> Vec<&'a str> {
    steps(document, job).iter().filter_map(|step| step["run"].as_str()).collect()
}

#[test]
fn release_h2_and_bootstrap_consume_the_same_staged_archive_without_product_rebuilds() {
    let document = workflow(".github/workflows/release.yml");
    let needs = document["jobs"]["prepare-h2"]["needs"].as_vec().expect("prepare dependencies");
    assert_eq!(
        needs,
        &[Yaml::String("assemble".into()), Yaml::String("build-h2-controller".into())]
    );
    assert_eq!(
        commands(&document, "prepare-h2"),
        ["cargo run --locked --package xtask -- release-qualification-prepare",]
    );
    let downloads = steps(&document, "prepare-h2")
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|value| value.starts_with("actions/download-artifact@"))
        })
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 2);
    assert_eq!(
        downloads[0]["with"]["name"].as_str(),
        Some("release-package-primary-${{ matrix.os }}")
    );
    assert_eq!(downloads[0]["with"]["path"].as_str(), Some("target/release-input"));
    assert_eq!(downloads[0]["with"].as_hash().expect("same-run options").len(), 2);
    assert_eq!(
        downloads[1]["with"]["pattern"].as_str(),
        Some("release-h2-tool-${{ matrix.os }}--*")
    );
    assert_eq!(downloads[1]["with"]["path"].as_str(), Some("target/debug"));
    assert_eq!(downloads[1]["with"].as_hash().expect("same-run options").len(), 3);
    for (job, command) in [
        (
            "h2",
            "cargo run --locked --package xtask -- product-native-qualification-prepared-shard ${{ matrix.shard }}",
        ),
        ("bootstrap", "cargo run --locked --package xtask -- release-bootstrap-staged-smoke"),
    ] {
        assert_eq!(document["jobs"][job]["needs"].as_str(), Some("prepare-h2"));
        let downloads = steps(&document, job)
            .iter()
            .filter(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("actions/download-artifact@"))
            })
            .collect::<Vec<_>>();
        assert_eq!(downloads.len(), 1);
        assert_eq!(
            downloads[0]["with"]["name"].as_str(),
            Some("release-h2-prepared-${{ matrix.os }}")
        );
        assert_eq!(downloads[0]["with"]["path"].as_str(), Some("target/prepared-h2"));
        assert_eq!(downloads[0]["with"].as_hash().expect("same-run options").len(), 2);
        let cargo = commands(&document, job)
            .into_iter()
            .filter(|value| value.starts_with("cargo "))
            .collect::<Vec<_>>();
        assert_eq!(
            cargo,
            ["cargo run --locked --package xtask -- product-native-qualification-restore", command]
        );
    }
}

#[test]
fn release_retains_every_h2_scenario_and_only_builds_the_two_native_controllers() {
    let document = workflow(".github/workflows/release.yml");
    let binaries = document["jobs"]["build-h2-controller"]["strategy"]["matrix"]["binary"]
        .as_vec()
        .expect("tools");
    assert_eq!(
        binaries,
        &[Yaml::String("peritus-h2".into()), Yaml::String("peritus-h2-controller".into())]
    );
    assert_eq!(
        commands(&document, "build-h2-controller"),
        [
            "cargo build --locked --package peritus-platform-qualification --bin ${{ matrix.binary }}",
        ]
    );
    let shards = document["jobs"]["h2"]["strategy"]["matrix"]["shard"].as_vec().expect("scenarios");
    assert_eq!(shards.len(), crate::product_package::H2_SHARD_COUNT);
    for (index, shard) in shards.iter().enumerate() {
        assert_eq!(shard.as_i64(), Some(i64::try_from(index).expect("index")));
    }
    for job in ["prepare-h2", "build-h2-controller", "h2", "bootstrap"] {
        assert_eq!(document["jobs"][job]["timeout-minutes"].as_i64(), Some(10));
        assert_eq!(document["jobs"][job]["strategy"]["fail-fast"].as_bool(), Some(false));
    }
    let retention = steps(&document, "h2").last().expect("evidence retention");
    assert_eq!(retention["if"].as_str(), Some("${{ always() }}"));
    assert_eq!(retention["with"]["path"].as_str(), Some("target/peritus-qualification/h2/**"));
    assert_eq!(retention["with"]["retention-days"].as_i64(), Some(30));
}
