use super::read_scope;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("peritus-formal-inventory-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).expect("create audit fixture");
        Self(root)
    }

    fn write(&self, shard: &str, bytes: &[u8]) {
        let directory = self.0.join("target/formal-scope").join(shard);
        fs::create_dir_all(&directory).expect("create scope directory");
        fs::write(directory.join("peritus-fixture-selected-functions.json"), bytes)
            .expect("write observed scope");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unavailable_scope_is_distinct_from_an_observed_zero_query_package() {
    let fixture = Fixture::new();
    assert!(read_scope(&fixture.0, "peritus-fixture").expect("absent scope").is_none());
    let scope = scope();
    fixture.write("foundation-state", &serde_json::to_vec(&scope).expect("scope JSON"));
    assert_eq!(read_scope(&fixture.0, "peritus-fixture").expect("observed scope"), Some(scope));
}

fn scope() -> serde_json::Value {
    json!({
        "package": "peritus-fixture",
        "root_targets": ["peritus_fixture"],
        "registered_symbols": 0,
        "queried_functions": {},
        "selected_functions": [],
        "has_solver_queries": false
    })
}

#[test]
fn multiple_shards_cannot_supply_an_ambiguous_package_observation() {
    let fixture = Fixture::new();
    let scope = serde_json::to_vec(&scope()).expect("scope JSON");
    fixture.write("foundation-state", &scope);
    fixture.write("app-runner", &scope);
    assert!(read_scope(&fixture.0, "peritus-fixture").is_err());
}

#[test]
fn partial_or_contradictory_observations_are_rejected() {
    let fixture = Fixture::new();
    let mut missing_queries = scope();
    missing_queries.as_object_mut().expect("scope fields").remove("queried_functions");
    let mut false_query_claim = scope();
    false_query_claim["has_solver_queries"] = json!(true);
    let mut foreign_query = scope();
    foreign_query["has_solver_queries"] = json!(true);
    foreign_query["selected_functions"] = json!(["peritus_other::check"]);
    foreign_query["queried_functions"] = json!({"peritus_other::check": "exec"});
    for malformed in
        [json!({"package": "peritus-fixture"}), missing_queries, false_query_claim, foreign_query]
    {
        fixture.write("foundation-state", &serde_json::to_vec(&malformed).expect("bad scope"));
        assert!(read_scope(&fixture.0, "peritus-fixture").is_err());
    }
}

#[test]
fn foreign_selected_specifications_cannot_fill_a_zero_query_observation() {
    let fixture = Fixture::new();
    let mut foreign_specification = scope();
    foreign_specification["registered_symbols"] = json!(1);
    foreign_specification["selected_functions"] = json!(["foreign_crate::spec"]);
    fixture.write(
        "foundation-state",
        &serde_json::to_vec(&foreign_specification).expect("foreign specification scope"),
    );
    assert!(read_scope(&fixture.0, "peritus-fixture").is_err());
}

#[test]
fn malformed_or_misidentified_observations_do_not_become_missing_scope() {
    let fixture = Fixture::new();
    for bytes in [b"partial JSON".as_slice(), br#"{"package":"peritus-other"}"#, b"{}"] {
        fixture.write("foundation-state", bytes);
        assert!(read_scope(&fixture.0, "peritus-fixture").is_err());
    }
}
