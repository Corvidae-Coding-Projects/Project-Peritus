use super::*;
use yaml_rust2::{Yaml, YamlLoader};

fn complete_release() -> Release {
    Release {
        tag_name: "v1.2.3".to_owned(),
        is_draft: true,
        assets: expected_assets(true)
            .into_iter()
            .map(|name| Asset { name, size: 1, state: "uploaded".to_owned() })
            .collect(),
    }
}

#[test]
fn publication_requires_every_target_and_evidence_asset() {
    let release = complete_release();
    assert_eq!(release.assets.len(), 52);
    validate_release(&release, "v1.2.3", true).expect("complete release");
    for index in 0..release.assets.len() {
        let mut missing = complete_release();
        missing.assets.remove(index);
        assert!(validate_release(&missing, "v1.2.3", true).is_err(), "missing asset {index}");
    }
}

#[test]
fn publication_rejects_empty_partial_duplicate_or_published_assets() {
    let mut release = complete_release();
    release.assets[0].size = 0;
    assert!(validate_release(&release, "v1.2.3", true).is_err());
    release.assets[0].size = 1;
    release.assets[0].state = "new".to_owned();
    assert!(validate_release(&release, "v1.2.3", true).is_err());
    release.assets[0].state = "uploaded".to_owned();
    release.assets.push(release.assets[0].clone());
    assert!(validate_release(&release, "v1.2.3", true).is_err());
    release.assets.pop();
    release.is_draft = false;
    assert!(validate_release(&release, "v1.2.3", true).is_err());
    release.is_draft = true;
    assert!(validate_release(&release, "v9.9.9", true).is_err());
}

#[test]
fn rendering_binds_both_bootstraps_without_changing_the_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    for name in BOOTSTRAPS {
        let source = fs::read_to_string(root.join(name)).expect("bootstrap source");
        let rendered = render_bootstrap(&source, "v1.2.3").expect("render");
        assert!(!rendered.contains(TAG_TOKEN));
        assert!(rendered.contains("'v1.2.3'"));
        assert!(source.contains(TAG_TOKEN));
    }
    for tag in ["latest", "v1.2", "v1.2.3.4", "v1..3", "v1.2.3';exit 0", "v1.2.3-rc1"] {
        assert!(render_bootstrap(TAG_TOKEN, tag).is_err());
    }
    assert!(render_bootstrap("no token", "v1.2.3").is_err());
    assert!(render_bootstrap(&format!("{TAG_TOKEN}{TAG_TOKEN}"), "v1.2.3").is_err());
}

fn workflow(path: &str) -> Yaml {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let source = fs::read_to_string(root.join(path)).expect("workflow");
    YamlLoader::load_from_str(&source).expect("YAML").remove(0)
}

#[test]
fn release_and_lifecycle_matrices_cover_each_native_target_once() {
    let release = workflow(".github/workflows/release.yml");
    let product = workflow(".github/workflows/product-package.yml");
    let expected =
        TARGETS.iter().map(|(_, _, runner)| *runner).collect::<std::collections::BTreeSet<_>>();
    for (document, job) in [
        (&release, "bootstrap"),
        (&release, "assemble"),
        (&release, "attest"),
        (&product, "bootstrap"),
        (&product, "h2"),
    ] {
        let os = document["jobs"][job]["strategy"]["matrix"]["os"].as_vec().expect("OS matrix");
        assert_eq!(os.len(), expected.len(), "{job} must not duplicate targets");
        let actual = os
            .iter()
            .map(|value| value.as_str().expect("runner"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(actual, expected, "target coverage in {job}");
    }
    let binaries = release["jobs"]["build-binary"]["strategy"]["matrix"]["include"]
        .as_vec()
        .expect("binary matrix");
    assert_eq!(binaries.len(), TARGETS.len() * 4);
    for (platform, _, runner) in TARGETS {
        for (package, binary) in [
            ("peritus-cli".to_owned(), "peritus".to_owned()),
            ("peritus-daemon".to_owned(), "peritusd".to_owned()),
            ("peritus-tui".to_owned(), "peritus-tui".to_owned()),
            (format!("peritus-sandbox-{platform}"), format!("peritus-{platform}-sandbox-helper")),
        ] {
            let artifact = format!("{binary}{}", if platform == "windows" { ".exe" } else { "" });
            assert_eq!(
                binaries
                    .iter()
                    .filter(|row| {
                        row["os"].as_str() == Some(runner)
                            && row["package"].as_str() == Some(package.as_str())
                            && row["binary"].as_str() == Some(binary.as_str())
                            && row["artifact"].as_str() == Some(artifact.as_str())
                    })
                    .count(),
                1,
                "{runner}/{binary} must have exactly one native build"
            );
        }
    }
}
