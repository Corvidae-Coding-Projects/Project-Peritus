//! Expand the real artifact templates before testing native target isolation.

use std::collections::BTreeSet;

use super::*;

fn transfer<'a>(document: &'a Yaml, job: &str, path: &str) -> &'a Yaml {
    let matches = document["jobs"][job]["steps"]
        .as_vec()
        .expect("steps")
        .iter()
        .filter(|step| step["with"]["path"].as_str() == Some(path))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one transfer for {job}/{path}");
    &matches[0]["with"]
}

fn expand(template: &str, role: &str, runner: &str, binary: &str) -> String {
    let result = template
        .replace("${{ matrix.build }}", role)
        .replace("${{ matrix.target.os }}", runner)
        .replace("${{ matrix.target.binary }}", binary)
        .replace("${{ matrix.os }}", runner)
        .replace("${{ matrix.binary }}", binary);
    assert!(!result.contains("${{"), "unhandled artifact template: {result}");
    result
}

fn selected<'a>(artifacts: &'a BTreeSet<String>, pattern: &str) -> BTreeSet<&'a str> {
    let prefix = pattern.strip_suffix('*').expect("one trailing wildcard");
    assert!(!prefix.contains(['*', '?', '[', ']']), "literal artifact prefix");
    artifacts.iter().filter(|name| name.starts_with(prefix)).map(String::as_str).collect()
}

#[test]
fn release_binary_downloads_match_only_their_platform_and_role() {
    let document = workflow(".github/workflows/release.yml");
    let upload =
        transfer(&document, "build-binary", "target/release/${{ matrix.target.artifact }}");
    let template = upload["name"].as_str().expect("binary artifact name");
    let download = transfer(&document, "assemble", "target/release");
    let pattern = download["pattern"].as_str().expect("binary artifact pattern");
    let rows = document["jobs"]["build-binary"]["strategy"]["matrix"]["target"]
        .as_vec()
        .expect("native binary producers");
    let mut artifacts = BTreeSet::new();
    for role in ["primary", "independent"] {
        for row in rows {
            let runner = row["os"].as_str().expect("producer runner");
            let binary = row["binary"].as_str().expect("producer binary");
            assert!(artifacts.insert(expand(template, role, runner, binary)));
        }
    }
    assert_eq!(artifacts.len(), TARGETS.len() * 4 * 2);
    for role in ["primary", "independent"] {
        for (platform, _, runner) in TARGETS {
            let helper = format!("peritus-{platform}-sandbox-helper");
            let expected = ["peritus", "peritusd", "peritus-tui", helper.as_str()]
                .map(|binary| expand(template, role, runner, binary));
            let expected = expected.iter().map(String::as_str).collect::<BTreeSet<_>>();
            assert_eq!(
                selected(&artifacts, &expand(pattern, role, runner, "")),
                expected,
                "download must select exactly four binaries for {role}/{runner}"
            );
        }
    }
}

#[test]
fn release_tool_downloads_match_only_their_platform() {
    let document = workflow(".github/workflows/release.yml");
    let upload = transfer(
        &document,
        "build-h2-controller",
        "target/debug/${{ matrix.binary }}${{ runner.os == 'Windows' && '.exe' || '' }}",
    );
    let template = upload["name"].as_str().expect("tool artifact name");
    let download = transfer(&document, "prepare-h2", "target/debug");
    let pattern = download["pattern"].as_str().expect("tool artifact pattern");
    let matrix = &document["jobs"]["build-h2-controller"]["strategy"]["matrix"];
    let mut artifacts = BTreeSet::new();
    for runner in matrix["os"].as_vec().expect("tool runners") {
        for binary in matrix["binary"].as_vec().expect("tools") {
            assert!(artifacts.insert(expand(
                template,
                "",
                runner.as_str().expect("runner"),
                binary.as_str().expect("tool"),
            )));
        }
    }
    assert_eq!(artifacts.len(), TARGETS.len() * 2);
    for (_, _, runner) in TARGETS {
        let expected = ["peritus-h2", "peritus-h2-controller"]
            .map(|binary| expand(template, "", runner, binary));
        let expected = expected.iter().map(String::as_str).collect::<BTreeSet<_>>();
        assert_eq!(
            selected(&artifacts, &expand(pattern, "", runner, "")),
            expected,
            "download must select exactly two qualification tools for {runner}"
        );
    }
}
