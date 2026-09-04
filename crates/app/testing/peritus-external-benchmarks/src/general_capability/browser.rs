use html5ever::{
    ParseOpts, parse_document,
    serialize::{SerializeOpts, serialize},
    tendril::TendrilSink as _,
};
use markup5ever_rcdom::{RcDom, SerializableHandle};
use peritus_obligations::{BrowserEvidence, BrowserImplementation, BrowserRequirement};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::fixture::{Expected, binding};

const CASES: &str = include_str!("../../tests/fixtures/general-capability/browser/cases.json");

#[derive(Deserialize)]
struct Fixture {
    html: String,
    standards_output: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    implementation: Implementation,
    oracle_passed: bool,
    expected: Expected,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Implementation {
    StandardsCompliant,
    ParserOnly,
}

#[test]
fn malformed_html_requires_a_standards_parser_oracle() {
    let fixture: Fixture = serde_json::from_str(CASES).expect("browser fixtures");
    let standards = standards_parse(&fixture.html);
    let hand_parser = fixture.html.replace("<table>", "<html><body><table>") + "</body></html>";
    assert_eq!(standards, fixture.standards_output);
    assert_ne!(standards, hand_parser);
    assert!(standards.contains("<tbody>"), "standards parser must repair table structure");

    let oracle = sha256(fixture.standards_output.as_bytes());
    let requirement = BrowserRequirement::new(oracle);
    for case in fixture.cases {
        let implementation = match case.implementation {
            Implementation::StandardsCompliant => BrowserImplementation::StandardsCompliant,
            Implementation::ParserOnly => BrowserImplementation::ParserOnly,
        };
        let evidence = BrowserEvidence::new(
            binding(51, 52),
            implementation,
            (matches!(implementation, BrowserImplementation::StandardsCompliant)).then_some(oracle),
            case.oracle_passed,
        );
        assert_eq!(
            evidence.satisfies(requirement),
            case.expected == Expected::Success,
            "{}",
            case.name
        );
    }
}

fn standards_parse(input: &str) -> String {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(input);
    let document = SerializableHandle::from(dom.document);
    let mut output = Vec::new();
    serialize(&mut output, &document, SerializeOpts::default()).expect("serialize parsed document");
    String::from_utf8(output).expect("serialized UTF-8")
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}
