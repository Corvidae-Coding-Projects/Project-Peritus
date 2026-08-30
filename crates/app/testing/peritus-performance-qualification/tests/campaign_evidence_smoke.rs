//! Operator-invoked end-to-end campaign and atomic evidence smoke.

#![cfg(unix)]

use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use peritus_benchmarks::{
    DatasetLimits, QualificationDataset, RunnerDescriptor, Sha256Digest, StableId,
};
use peritus_performance_qualification::{
    CampaignCoordinator, CampaignEvidenceWriter, CampaignMode, CampaignRequest, MachineObservation,
};
use sha2::{Digest as _, Sha256};

const PROFILE: &str = r#"{
  "schema_version": 1,
  "id": "smoke-profile",
  "description": "One-operation evidence smoke",
  "reference_machine": {
    "operating_system": "smoke-os",
    "architecture": "smoke-arch",
    "cpu_model": "smoke cpu",
    "logical_cores": 1,
    "memory_bytes": 1048576,
    "storage_class": "smoke-storage"
  },
  "resource_envelope": {
    "max_active_runs": 1,
    "max_active_processes": 1,
    "max_provider_requests": 1,
    "max_memory_bytes": 1048576,
    "max_disk_bytes": 1048576,
    "max_tokens": 1024,
    "command_queue_capacity": 4,
    "terminal_queue_capacity": 4,
    "exporter_queue_capacity": 4,
    "provider_queue_capacity": 4
  },
  "regression_policy": {
    "warning_basis_points": 500,
    "blocking_basis_points": 1000,
    "minimum_absolute_delta": 1,
    "baseline_required": false
  },
  "max_measurements": 64,
  "required_workloads": ["smoke-event"],
  "objectives": [
    {
      "id": "smoke-event-latency",
      "workload_id": "smoke-event",
      "metric": "event_append_latency",
      "statistic": "maximum",
      "bound": "at_most",
      "threshold": 10000000,
      "minimum_samples": 1
    }
  ]
}"#;

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
fn real_campaign_publishes_a_complete_atomic_bundle() {
    let daemon = daemon_executable();
    let runner_executable = std::env::current_exe().expect("test runner executable");
    let runner = RunnerDescriptor::new(
        id("peritus-h3-evidence-smoke"),
        env!("CARGO_PKG_VERSION"),
        digest_file(&runner_executable),
    )
    .expect("runner descriptor");
    let dataset =
        QualificationDataset::from_json(PROFILE, WORKLOADS, DatasetLimits::production_defaults())
            .expect("smoke dataset");
    let machine = MachineObservation::new(
        id("smoke-os"),
        id("smoke-arch"),
        "smoke cpu",
        1,
        1_048_576,
        id("smoke-storage"),
    )
    .expect("machine");
    let outcome = CampaignCoordinator::run(CampaignRequest::new(
        dataset,
        daemon,
        "operator-smoke",
        id("smoke-evidence-run"),
        runner,
        machine,
        CampaignMode::Load,
    ))
    .expect("campaign");

    let temporary = tempfile::tempdir().expect("evidence parent");
    let output = temporary.path().join("bundle");
    let published = CampaignEvidenceWriter::publish(
        &output,
        PROFILE,
        WORKLOADS,
        None,
        &runner_executable,
        &outcome,
    )
    .expect("evidence bundle");

    assert_eq!(published.root(), output);
    assert_eq!(published.manifest().artifacts().len(), 8);
    assert_eq!(
        published.report().evidence_manifest_digest(),
        &published.manifest().digest().expect("manifest digest")
    );
    assert_eq!(
        std::fs::read_to_string(output.join("inputs/profile.json")).expect("profile"),
        PROFILE
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
    ] {
        assert!(output.join(relative).is_file(), "missing {relative}");
    }
}

fn digest_file(path: &Path) -> Sha256Digest {
    let mut file = File::open(path).expect("open executable");
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer).expect("read executable");
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let bytes = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(encoded, "{byte:02x}").expect("write digest");
    }
    Sha256Digest::parse(encoded).expect("digest")
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
