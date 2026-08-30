//! Operator-invoked end-to-end campaign and atomic evidence smoke.

#![cfg(unix)]

use std::ffi::OsString;
use std::path::PathBuf;

use peritus_benchmarks::StableId;
use peritus_performance_qualification::{MachineProbe, OperatorOptions};

const WORKLOADS: &str = r#"{
  "schema_version": 1,
  "workloads": [
    {
      "id": "smoke-event",
      "description": "One event through public A3",
      "scenario": "event_append",
      "duration_seconds": 1,
      "operations_per_second": 1,
      "max_concurrency": 1,
      "payload_bytes": 4096,
      "memory_reservation_bytes": 1,
      "disk_reservation_bytes": 1,
      "token_reservation": 1,
      "queue_capacity": 1,
      "seed": 1
    }
  ]
}"#;

#[test]
#[ignore = "set PERITUS_H3_DAEMON to a built peritusd and invoke explicitly"]
fn real_operator_publishes_a_complete_atomic_bundle() {
    let temporary = tempfile::tempdir().expect("evidence parent");
    let profile = profile_document();
    let profile_path = temporary.path().join("profile.json");
    let workload_path = temporary.path().join("workloads.json");
    std::fs::write(&profile_path, &profile).expect("profile input");
    std::fs::write(&workload_path, WORKLOADS).expect("workload input");
    let output = temporary.path().join("bundle");
    let published = OperatorOptions::parse(vec![
        OsString::from("load"),
        OsString::from("--daemon"),
        daemon_executable().into_os_string(),
        OsString::from("--profile"),
        profile_path.clone().into_os_string(),
        OsString::from("--workloads"),
        workload_path.clone().into_os_string(),
        OsString::from("--evidence"),
        output.clone().into_os_string(),
        OsString::from("--storage-class"),
        OsString::from("smoke-storage"),
        OsString::from("--revision"),
        OsString::from("operator-smoke"),
    ])
    .expect("operator options")
    .execute()
    .expect("operator execution");

    assert_eq!(published.root(), output);
    assert_eq!(published.manifest().artifacts().len(), 8);
    assert!(published.baseline_candidate().is_some());
    assert!(published.baseline_candidate_digest().is_some());
    assert_eq!(
        published.report().evidence_manifest_digest(),
        &published.manifest().digest().expect("manifest digest")
    );
    assert_eq!(
        std::fs::read_to_string(output.join("inputs/profile.json")).expect("profile"),
        profile
    );
    for relative in [
        "identity/peritusd",
        "identity/qualification-runner",
        "results/measurements.ndjson",
        "results/receipts.json",
        "results/accounting.json",
        "results/machine.json",
        "manifest.json",
        "report.json",
        "baseline-candidate.json",
    ] {
        assert!(output.join(relative).is_file(), "missing {relative}");
    }

    let candidate_path = output.join("baseline-candidate.json");
    let candidate_digest =
        published.baseline_candidate_digest().expect("candidate digest").to_string();
    let accepted_output = temporary.path().join("accepted-run");
    let accepted = OperatorOptions::parse(vec![
        OsString::from("load"),
        OsString::from("--daemon"),
        daemon_executable().into_os_string(),
        OsString::from("--profile"),
        profile_path.into_os_string(),
        OsString::from("--workloads"),
        workload_path.into_os_string(),
        OsString::from("--baseline"),
        candidate_path.clone().into_os_string(),
        OsString::from("--accept-baseline-sha256"),
        OsString::from(candidate_digest),
        OsString::from("--evidence"),
        accepted_output.clone().into_os_string(),
        OsString::from("--storage-class"),
        OsString::from("smoke-storage"),
        OsString::from("--revision"),
        OsString::from("operator-smoke"),
    ])
    .expect("accepted operator options")
    .execute()
    .expect("accepted operator execution");
    assert!(accepted.baseline_candidate().is_some());
    assert_eq!(
        std::fs::read(accepted_output.join("inputs/accepted-baseline.json"))
            .expect("retained accepted baseline"),
        std::fs::read(candidate_path).expect("candidate baseline")
    );
}

fn profile_document() -> String {
    let machine = MachineProbe::observe(id("smoke-storage")).expect("machine probe");
    let class = machine.reference_machine();
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "id": "smoke-profile",
        "description": "One-operation operator and evidence smoke",
        "reference_machine": {
            "operating_system": class.operating_system().as_str(),
            "architecture": class.architecture().as_str(),
            "cpu_model": class.cpu_model(),
            "logical_cores": class.logical_cores(),
            "memory_bytes": class.memory_bytes(),
            "storage_class": class.storage_class().as_str()
        },
        "resource_envelope": {
            "max_active_runs": 1,
            "max_active_processes": 1,
            "max_provider_requests": 1,
            "max_memory_bytes": 1_048_576,
            "max_disk_bytes": 1_048_576,
            "max_tokens": 1_024,
            "command_queue_capacity": 4,
            "terminal_queue_capacity": 4,
            "exporter_queue_capacity": 4,
            "provider_queue_capacity": 4
        },
        "regression_policy": {
            "warning_basis_points": 500,
            "blocking_basis_points": 1_000,
            "minimum_absolute_delta": 1,
            "baseline_required": false
        },
        "max_measurements": 64,
        "required_workloads": ["smoke-event"],
        "objectives": [{
            "id": "smoke-event-latency",
            "workload_id": "smoke-event",
            "metric": "event_append_latency",
            "statistic": "maximum",
            "bound": "at_most",
            "threshold": 10_000_000,
            "minimum_samples": 1
        }]
    }))
    .expect("profile JSON")
}

fn daemon_executable() -> PathBuf {
    PathBuf::from(
        std::env::var_os("PERITUS_H3_DAEMON")
            .expect("PERITUS_H3_DAEMON must name the peritusd executable"),
    )
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("stable id")
}
