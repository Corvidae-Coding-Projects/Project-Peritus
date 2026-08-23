use super::manifest::{TrustedOccurrence, validate};
use super::manifest_impact::sha256_hex;
use super::manifest_support::version_is_pinned;
use super::manifest_symbol::validate_symbol;
use crate::model::{
    ArchitecturePolicy, CargoMetadata, CargoPackage, CargoPackageMetadata, CargoTarget,
    PackagePolicy, PeritusPackageMetadata,
};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("peritus-trust-manifest-{}-{id}", process::id()));
        fs::create_dir_all(&path).expect("fixture root must be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file must have a parent"))
            .expect("fixture directory must be created");
        fs::write(path, contents).expect("fixture file must be written");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

fn policy() -> ArchitecturePolicy {
    ArchitecturePolicy {
        schema: 3,
        soft_source_lines: 400,
        hard_source_lines: 700,
        root_module_lines: 80,
        required_license: "MIT".to_owned(),
        ignored_directories: Vec::new(),
        forbidden_module_names: Vec::new(),
        trusted_source_roots: vec![PathBuf::from("crates/foundation/peritus-tcb/src")],
        source_exceptions: Vec::new(),
        layers: Vec::new(),
        verification_classes: Vec::new(),
        forbidden_dependencies: Vec::new(),
        controlled_source_roots: Vec::new(),
        refinement_reservations: Vec::new(),
        packages: vec![
            PackagePolicy {
                name: "peritus-tcb".to_owned(),
                path: PathBuf::from("crates/foundation/peritus-tcb"),
                owner: "A1".to_owned(),
                layer: "foundation".to_owned(),
                verification_class: "T".to_owned(),
            },
            PackagePolicy {
                name: "peritus-types".to_owned(),
                path: PathBuf::from("crates/foundation/peritus-types"),
                owner: "A1".to_owned(),
                layer: "foundation".to_owned(),
                verification_class: "V".to_owned(),
            },
        ],
    }
}

fn cargo(fixture: &Fixture) -> CargoMetadata {
    CargoMetadata {
        workspace_members: vec!["tcb".to_owned(), "types".to_owned()],
        packages: [
            ("tcb", "peritus-tcb", "crates/foundation/peritus-tcb", "T"),
            ("types", "peritus-types", "crates/foundation/peritus-types", "V"),
        ]
        .into_iter()
        .map(|(id, name, root, class)| CargoPackage {
            id: id.to_owned(),
            name: name.to_owned(),
            version: "0.0.0".to_owned(),
            edition: "2024".to_owned(),
            rust_version: Some("1.97.1".to_owned()),
            license: Some("MIT".to_owned()),
            manifest_path: fixture.path().join(root).join("Cargo.toml"),
            readme: None,
            dependencies: Vec::new(),
            targets: vec![CargoTarget {
                kind: vec!["lib".to_owned()],
                crate_types: vec!["lib".to_owned()],
                src_path: fixture.path().join(root).join("src/lib.rs"),
            }],
            metadata: CargoPackageMetadata {
                peritus: Some(PeritusPackageMetadata {
                    owner: "A1".to_owned(),
                    layer: "foundation".to_owned(),
                    verification_class: class.to_owned(),
                }),
                verus: None,
            },
        })
        .collect(),
    }
}

fn write_fixture(fixture: &Fixture, trust_entries: &str) {
    for (path, contents) in [
        ("Cargo.toml", "[workspace]\nresolver='3'\n"),
        ("Cargo.lock", "version = 4\n"),
        (".cargo/config.toml", "[build]\nincremental = false\n"),
        ("rust-toolchain.toml", "[toolchain]\nchannel='1.97.1'\n"),
        ("toolchains.toml", "schema = 1\n"),
        ("architecture.toml", "schema = 3\n"),
        ("crates/foundation/peritus-tcb/Cargo.toml", "[package]\nname='peritus-tcb'\n"),
        ("crates/foundation/peritus-types/Cargo.toml", "[package]\nname='peritus-types'\n"),
        (
            "crates/foundation/peritus-tcb/src/lib.rs",
            "fn audited() { assume(false); }\n#[test]\nfn evidence_case() { let _value = 1; }\n",
        ),
        ("crates/foundation/peritus-types/src/lib.rs", "pub fn value() -> u64 { 1 }\n"),
    ] {
        fixture.write(path, contents);
    }
    let actor_provenance = r#"{
  "schema": "peritus.verification.actor-provenance",
  "schema_version": 1,
  "baseline": "A1",
  "entries": [
    {
      "actor_id": "ACTOR-0001",
      "kind": "crosslink-agent",
      "principal": "SHA256:eV8eZPaZxut5mrkihmvsOTrGWClwD+B/HR//do+oIeI",
      "repository": "Corvidae-Coding-Projects/Project-Peritus",
      "issue": 3,
      "issue_created_at": "2026-08-21T21:10:43.329647070Z",
      "session": 2,
      "task": "/root",
      "mode": "implementation",
      "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEKuTLQN4A79bLYizdWIYCfXTIaDgY2YWxHnZ7j5FftS fixture-owner",
      "allowed_signer": "6ME5@crosslink ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEKuTLQN4A79bLYizdWIYCfXTIaDgY2YWxHnZ7j5FftS fixture-owner",
      "record_locators": ["embedded:allowed-signer", "embedded:public-key"]
    },
    {
      "actor_id": "ACTOR-0002",
      "kind": "codex-subagent",
      "principal": "Corvidae-Coding-Projects/Project-Peritus/session/2/task/root/fixture_reviewer",
      "repository": "Corvidae-Coding-Projects/Project-Peritus",
      "issue": 3,
      "issue_created_at": "2026-08-21T21:10:43.329647070Z",
      "session": 2,
      "task": "/root/fixture_reviewer",
      "mode": "read-only-review",
      "model": "gpt-5.6-sol",
      "reasoning_effort": "xhigh",
      "record_locators": ["codex-collaboration:Corvidae-Coding-Projects/Project-Peritus/session/2/task/root/fixture_reviewer"]
    }
  ]
}
"#;
    fixture.write("verification/actor-provenance.json", actor_provenance);
    let provenance_sha256 = sha256_hex(actor_provenance.as_bytes());
    fixture.write(
        "verification/actors.toml",
        &format!(r#"schema = "peritus.verification.actors"
schema_version = 1
baseline = "A1"
[[entries]]
id = "ACTOR-0001"
kind = "crosslink-agent"
principal = "SHA256:eV8eZPaZxut5mrkihmvsOTrGWClwD+B/HR//do+oIeI"
display_name = "Fixture boundary owner"
roles = ["owner"]
provenance = {{ record_path = "verification/actor-provenance.json", record_sha256 = "{provenance_sha256}" }}
[[entries]]
id = "ACTOR-0002"
kind = "codex-subagent"
principal = "Corvidae-Coding-Projects/Project-Peritus/session/2/task/root/fixture_reviewer"
display_name = "Fixture independent reviewer"
roles = ["reviewer"]
provenance = {{ record_path = "verification/actor-provenance.json", record_sha256 = "{provenance_sha256}" }}
"#),
    );
    fixture.write(
        "verification/trust.toml",
        &format!(
            "schema = 'peritus.verification.trust'\nschema_version = 1\nbaseline = 'A1'\nentries = {trust_entries}\n"
        ),
    );
    fixture.write(
        "verification/exclusions.toml",
        "schema = 'peritus.verification.exclusions'\nschema_version = 1\nbaseline = 'A1'\nentries = []\n",
    );
    fixture.write(
        "verification/obligations.toml",
        "schema = 'peritus.verification.obligations'\nschema_version = 1\nbaseline = 'A1'\nentries = []\n",
    );
    write_proof_impact(fixture);
}

fn write_proof_impact(fixture: &Fixture) {
    let t = "[{ package = \"peritus-tcb\", verification_class = \"T\" }]";
    let v = "[{ package = \"peritus-types\", verification_class = \"V\" }]";
    let shared = "[{ package = \"peritus-tcb\", verification_class = \"T\" }, { package = \"peritus-types\", verification_class = \"V\" }]";
    let inputs = [
        (".cargo/config.toml", shared),
        ("Cargo.lock", shared),
        ("Cargo.toml", shared),
        ("architecture.toml", shared),
        ("crates/foundation/peritus-tcb/Cargo.toml", t),
        ("crates/foundation/peritus-tcb/src/lib.rs", t),
        ("crates/foundation/peritus-types/Cargo.toml", v),
        ("crates/foundation/peritus-types/src/lib.rs", v),
        ("rust-toolchain.toml", shared),
        ("toolchains.toml", shared),
        ("verification/actor-provenance.json", shared),
        ("verification/actors.toml", shared),
        ("verification/exclusions.toml", shared),
        ("verification/obligations.toml", shared),
        ("verification/trust.toml", shared),
    ];
    let mut source_records = String::new();
    let mut transitions = String::new();
    for (path, affected) in inputs {
        let digest = sha256_hex(
            &fs::read(fixture.path().join(path)).expect("fixture input must be readable"),
        );
        write!(
            source_records,
            "[[sources]]\nsource_file = \"{path}\"\nsha256 = \"{digest}\"\naffected_packages = {affected}\nchange_id = \"PCR-0001\"\n"
        )
        .expect("writing fixture TOML to a String cannot fail");
        write!(
            transitions,
            "[[changes.source_changes]]\nsource_file = \"{path}\"\ncurrent = {{ sha256 = \"{digest}\", affected_packages = {affected} }}\n"
        )
        .expect("writing fixture TOML to a String cannot fail");
    }
    fixture.write(
        "verification/proof-impact.toml",
        &format!(
            r#"schema = "peritus.verification.proof-impact"
schema_version = 1
baseline = "A1"
hash_algorithm = "sha256-raw-bytes-v1"
{source_records}[[changes]]
id = "PCR-0001"
status = "approved"
change_kinds = ["executable", "specification", "precondition", "postcondition", "proof"]
rationale = "establishes the exact fixture input baseline"
impact = "accounts for fixture semantics without claiming an invariant"
owner = "ACTOR-0001"
reviewer = "ACTOR-0002"
review_date = "2026-08-20"
{transitions}
[[changes.evidence]]
kind = "ordinary-test"
owning_crate = "peritus-tcb"
command = "cargo test --package peritus-tcb --all-targets --all-features --locked"
[[changes.evidence]]
kind = "verus-verify"
owning_crate = "peritus-tcb"
command = "cargo verus verify --package peritus-tcb --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20"
[[changes.evidence]]
kind = "ordinary-test"
owning_crate = "peritus-types"
command = "cargo test --package peritus-types --all-targets --all-features --locked"
[[changes.evidence]]
kind = "verus-verify"
owning_crate = "peritus-types"
command = "cargo verus verify --package peritus-types --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20"
"#
        ),
    );
}

fn sources(fixture: &Fixture) -> Vec<PathBuf> {
    vec![
        fixture.path().join("crates/foundation/peritus-tcb/src/lib.rs"),
        fixture.path().join("crates/foundation/peritus-types/src/lib.rs"),
    ]
}

fn validate_fixture(
    fixture: &Fixture,
    diagnostics: &mut Vec<crate::error::Diagnostic>,
) -> Result<(), crate::error::XtaskError> {
    validate(fixture.path(), &policy(), &cargo(fixture), &sources(fixture), &[], false, diagnostics)
}

fn trust_entry() -> &'static str {
    r##"[{
id = "TRUST-0001",
symbol = "peritus_tcb::audited",
owning_crate = "peritus-tcb",
source_file = "crates/foundation/peritus-tcb/src/lib.rs",
source_line = 1,
construct_kind = "assume",
upstream = "fixture ABI",
upstream_version = "1.2.3",
assumed_contract = "returns the documented fixture observation",
threat_if_false = "invalidates the fixture refinement boundary",
evidence = [{ kind = "refinement-test", source_file = "crates/foundation/peritus-tcb/src/lib.rs", symbol = "peritus_tcb::evidence_case", command = "cargo test --package peritus-tcb --all-targets --all-features --locked" }],
live_issue = "#3",
owner = "ACTOR-0001",
reviewer = "ACTOR-0002",
review_date = "2026-08-20",
expiry_date = "2099-08-20"
}]"##
}

fn exclusion_entry() -> &'static str {
    r##"[{
id = "EXCL-0001",
symbol = "peritus_tcb::evidence_case",
owning_crate = "peritus-tcb",
source_file = "crates/foundation/peritus-tcb/src/lib.rs",
source_line = 3,
verification_class = "T",
unsupported_feature = "pinned Verus cannot express the fixture boundary",
risk = "the fixture observation remains outside formal proof",
evidence = [{ kind = "conformance-test", source_file = "crates/foundation/peritus-tcb/src/lib.rs", symbol = "peritus_tcb::evidence_case", command = "cargo test --package peritus-tcb --all-targets --all-features --locked" }],
live_issue = "#3",
owner = "ACTOR-0001",
reviewer = "ACTOR-0002",
review_date = "2026-08-20",
upstream_tracking = "https://example.invalid/verus/feature/1",
revisit_plan = "replace the exclusion when pinned Verus supports the fixture boundary",
revisit_by = "2099-08-20"
}]"##
}

fn excluded_obligation() -> &'static str {
    r##"[{
id = "OBL-0001",
kind = "contract",
statement = "the fixture evidence symbol remains governed by its explicit exclusion",
owning_crate = "peritus-tcb",
source_file = "crates/foundation/peritus-tcb/src/lib.rs",
symbol = "peritus_tcb::evidence_case",
status = "excluded",
dependencies = [],
live_issue = "#3",
owner = "ACTOR-0001",
evidence = [],
exclusion_id = "EXCL-0001"
}]"##
}

fn write_coverage_documents(fixture: &Fixture, exclusions: &str, obligations: &str) {
    fixture.write(
        "verification/exclusions.toml",
        &format!(
            "schema = 'peritus.verification.exclusions'\nschema_version = 1\nbaseline = 'A1'\nentries = {exclusions}\n"
        ),
    );
    fixture.write(
        "verification/obligations.toml",
        &format!(
            "schema = 'peritus.verification.obligations'\nschema_version = 1\nbaseline = 'A1'\nentries = {obligations}\n"
        ),
    );
    write_proof_impact(fixture);
}

fn occurrence() -> TrustedOccurrence {
    TrustedOccurrence {
        source: PathBuf::from("crates/foundation/peritus-tcb/src/lib.rs"),
        line: 1,
        construct: "assume",
        symbol: "peritus_tcb::audited".to_owned(),
    }
}

#[path = "manifest_tests/adversarial.rs"]
mod adversarial;
#[path = "manifest_tests/core.rs"]
mod core;
