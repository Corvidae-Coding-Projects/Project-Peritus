//! Windows release determinism belongs to final native linking, not byte rewriting.

use super::*;

#[test]
fn windows_release_binaries_request_determinism_only_from_the_final_native_linker() {
    let document = workflow(".github/workflows/release.yml");
    let build = &document["jobs"]["build-binary"];
    let steps = build["steps"].as_vec().expect("binary steps");
    let native = steps
        .iter()
        .filter(|step| step["if"].as_str() == Some("${{ runner.os == 'Windows' }}"))
        .collect::<Vec<_>>();
    assert_eq!(native.len(), 1, "one deterministic native Windows compilation step");
    assert!(native[0]["env"].is_badvalue(), "no ambient compiler or linker overrides");
    assert_eq!(
        native[0]["run"].as_str(),
        Some(
            "cargo rustc --release --locked --package ${{ matrix.target.package }} --bin ${{ matrix.target.binary }} -- -C link-arg=/Brepro"
        )
    );
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
