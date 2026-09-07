//! Windows release determinism belongs to native compilation and linking, not byte rewriting.

use super::*;

#[test]
fn windows_release_binaries_use_reviewed_native_compilation_and_deterministic_linking() {
    let document = workflow(".github/workflows/release.yml");
    let build = &document["jobs"]["build-binary"];
    let steps = build["steps"].as_vec().expect("binary steps");
    for (condition, command) in [
        (
            "${{ matrix.target.os == 'windows-2025' }}",
            "cargo run --locked --package xtask -- release-windows-binary",
        ),
        (
            "${{ matrix.target.os == 'windows-11-arm' }}",
            "cargo rustc --release --locked --package ${{ matrix.target.package }} --bin ${{ matrix.target.binary }} -- -C link-arg=/Brepro",
        ),
    ] {
        let native =
            steps.iter().filter(|step| step["if"].as_str() == Some(condition)).collect::<Vec<_>>();
        assert_eq!(native.len(), 1, "one reviewed compilation step per native Windows target");
        if command.ends_with("release-windows-binary") {
            assert_eq!(native[0]["env"].as_hash().expect("binary selection").len(), 1);
            assert_eq!(
                native[0]["env"]["PERITUS_RELEASE_BINARY"].as_str(),
                Some("${{ matrix.target.binary }}")
            );
        } else {
            assert!(native[0]["env"].is_badvalue(), "no ambient compiler or linker overrides");
        }
        assert_eq!(native[0]["run"].as_str(), Some(command));
    }
    assert_eq!(build["runs-on"].as_str(), Some("${{ matrix.target.os }}"));
    for runner in ["windows-2025", "windows-11-arm"] {
        let rows = build["strategy"]["matrix"]["target"]
            .as_vec()
            .expect("binary matrix")
            .iter()
            .filter(|row| row["os"].as_str() == Some(runner))
            .map(|row| {
                (row["package"].as_str().expect("package"), row["binary"].as_str().expect("binary"))
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            rows,
            [
                ("peritus-cli", "peritus"),
                ("peritus-daemon", "peritusd"),
                ("peritus-tui", "peritus-tui"),
                ("peritus-sandbox-windows", "peritus-windows-sandbox-helper"),
            ]
            .into_iter()
            .collect(),
            "all four products must be compiled natively on {runner}"
        );
    }
    assert_eq!(
        build["strategy"]["matrix"]["build"].as_vec().expect("independent roles"),
        &[Yaml::String("primary".into()), Yaml::String("independent".into())]
    );
}
