//! End-to-end fixture checks for Terminal-Bench report construction and publication.

use std::{ffi::OsString, fs, path::PathBuf};

use serde_json::{Value, json};
use tempfile::TempDir;

use super::{
    build,
    model::{CampaignMode, IdentityPolicy, ReportRequest},
    run_cli,
};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Fixture {
    _temporary: TempDir,
    job: PathBuf,
    pin: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new(total: usize, completed: usize, finished: bool) -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let job = temporary.path().join("job");
        fs::create_dir(&job).expect("job directory");
        let pin = temporary.path().join("pin.toml");
        fs::write(&pin, "schema_version = 1\ndataset = \"terminal-bench-2\"\n").expect("pin file");
        write_json(
            &job.join("result.json"),
            &json!({
                "id": "job-id",
                "started_at": "2026-08-30T00:00:00Z",
                "updated_at": "2026-08-30T00:01:00Z",
                "finished_at": finished.then_some("2026-08-30T00:02:00Z"),
                "n_total_trials": total,
                "stats": {
                    "n_completed_trials": completed,
                    "n_errored_trials": 0,
                    "n_running_trials": usize::from(!finished),
                    "n_pending_trials": total.saturating_sub(completed + usize::from(!finished)),
                    "n_cancelled_trials": 0,
                    "n_retries": 2,
                    "ignored_upstream_field": true
                }
            }),
        );
        Self { output: temporary.path().join("report.json"), _temporary: temporary, job, pin }
    }

    fn add_trial(&self, name: &str, reward: Option<f64>) {
        let directory = self.job.join(name);
        fs::create_dir_all(directory.join("agent/peritus")).expect("trial evidence directory");
        fs::create_dir_all(directory.join("verifier")).expect("verifier directory");
        write_json(
            &directory.join("result.json"),
            &json!({
                "trial_name": name,
                "task_name": "terminal-bench/example",
                "task_id": {"ref": "sha256:task"},
                "source": "terminal-bench/terminal-bench-2",
                "task_checksum": "task-checksum",
                "agent_info": {
                    "name": "peritus",
                    "version": "0.0.0",
                    "model_info": {"name": "test-model", "provider": "peritus"}
                },
                "agent_result": {
                    "n_input_tokens": 10,
                    "n_cache_tokens": 4,
                    "n_output_tokens": 3,
                    "cost_usd": null,
                    "metadata": {
                        "peritus_product_accepted": true,
                        "peritus_failure_kind": null,
                        "peritus_requests": 2,
                        "peritus_agent_source_revision": REVISION,
                        "peritus_agent_binary_sha256": DIGEST
                    }
                },
                "reward": 0.0,
                "verifier_result": {"rewards": {"reward": reward}},
                "exception_info": null,
                "started_at": "2026-08-30T00:00:00Z",
                "finished_at": "2026-08-30T00:01:00Z"
            }),
        );
        write_json(
            &directory.join("agent/peritus/invocation.json"),
            &json!({
                "schema_version": 2,
                "success": true,
                "task_id": "example",
                "session_id": "session-id",
                "harness_model_id": "peritus/test-model",
                "writer": "writer",
                "reviewer": "reviewer",
                "elapsed_ms": 100,
                "usage": {
                    "requests": 2,
                    "input_tokens": 10,
                    "cached_input_tokens": 4,
                    "output_tokens": 3,
                    "total_tokens": 13,
                    "provider_cost_microunits": 0
                },
                "failure_kind": null,
                "failure": null
            }),
        );
        fs::write(directory.join("verifier/test-stdout.txt"), "passed\n").expect("verifier output");
    }

    fn remove_trial_identity(&self, name: &str) {
        let path = self.job.join(name).join("result.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&path).expect("trial result"))
            .expect("trial result JSON");
        let metadata = value["agent_result"]["metadata"].as_object_mut().expect("metadata object");
        metadata.remove("peritus_agent_source_revision");
        metadata.remove("peritus_agent_binary_sha256");
        write_json(&path, &value);
    }

    fn remove_trial_usage(&self, name: &str) {
        let path = self.job.join(name).join("result.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&path).expect("trial result"))
            .expect("trial result JSON");
        value["agent_result"] = Value::Null;
        write_json(&path, &value);
    }

    fn request(
        &self,
        mode: CampaignMode,
        expected_trials: usize,
        identity_policy: IdentityPolicy,
    ) -> ReportRequest {
        ReportRequest {
            job_directory: self.job.clone(),
            output: self.output.clone(),
            pin_file: self.pin.clone(),
            expected_trials,
            mode,
            campaign_label: "frozen-baseline".to_owned(),
            identity_policy,
            agent_sha256: DIGEST.to_owned(),
        }
    }

    fn arguments(&self, mode: &str, expected_trials: usize) -> Vec<OsString> {
        [
            "peritus-terminalbench-report".to_owned(),
            "--job-dir".to_owned(),
            self.job.display().to_string(),
            "--output".to_owned(),
            self.output.display().to_string(),
            "--pin-file".to_owned(),
            self.pin.display().to_string(),
            "--expected-trials".to_owned(),
            expected_trials.to_string(),
            "--mode".to_owned(),
            mode.to_owned(),
            "--campaign-label".to_owned(),
            "frozen-baseline".to_owned(),
            "--identity-policy".to_owned(),
            "require-native".to_owned(),
            "--agent-sha256".to_owned(),
            DIGEST.to_owned(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }
}

#[test]
fn snapshot_uses_direct_child_results_and_nested_verifier_reward() {
    let fixture = Fixture::new(2, 1, false);
    fixture.add_trial("example__one", Some(1.0));

    let report = build(&fixture.request(CampaignMode::Snapshot, 2, IdentityPolicy::RequireNative))
        .expect("snapshot report");
    let value = serde_json::to_value(report).expect("serialized report");

    assert_eq!(value["complete"], false);
    assert_eq!(value["state"]["n_completed_trials"], 1);
    assert_eq!(value["aggregate"]["reward_one"], 1);
    assert_eq!(value["aggregate"]["reward_zero"], 0);
    assert_eq!(value["aggregate"]["scored_accuracy"], 1.0);
    assert_eq!(value["aggregate"]["harbor_retries"], 2);
    assert_eq!(value["trials"][0]["evidence"]["harbor_result"], "example__one/result.json");
}

#[test]
fn rejects_job_state_child_result_publication_race() {
    let fixture = Fixture::new(2, 2, false);
    fixture.add_trial("example__one", Some(1.0));

    let error = build(&fixture.request(CampaignMode::Snapshot, 2, IdentityPolicy::RequireNative))
        .expect_err("inconsistent job");
    assert!(error.to_string().contains("2 completed trials but 1 child result"));
}

#[test]
fn final_mode_refuses_an_incomplete_campaign() {
    let fixture = Fixture::new(2, 1, false);
    fixture.add_trial("example__one", Some(1.0));

    let error = build(&fixture.request(CampaignMode::Final, 2, IdentityPolicy::RequireNative))
        .expect_err("incomplete final");
    assert!(error.to_string().contains("final report requires a finished job"));
}

#[test]
fn command_publishes_atomically_and_never_overwrites() {
    let fixture = Fixture::new(1, 1, true);
    fixture.add_trial("example__one", Some(1.0));

    run_cli(fixture.arguments("final", 1)).expect("published report");
    let value: Value = serde_json::from_slice(&fs::read(&fixture.output).expect("report bytes"))
        .expect("report JSON");
    assert_eq!(value["complete"], true);
    assert_eq!(value["aggregate"]["completed_success_rate"], 1.0);
    assert!(!fixture.output.with_extension("json.new").exists());

    let error = run_cli(fixture.arguments("final", 1)).expect_err("existing report is immutable");
    assert!(error.to_string().contains("report output already exists"));
}

#[test]
fn legacy_policy_exposes_missing_source_identity_without_inventing_it() {
    let fixture = Fixture::new(1, 1, true);
    fixture.add_trial("example__one", Some(1.0));
    fixture.remove_trial_identity("example__one");

    let report = build(&fixture.request(CampaignMode::Final, 1, IdentityPolicy::AllowLegacy))
        .expect("legacy report");
    let value = serde_json::to_value(report).expect("serialized report");
    assert_eq!(value["agent"]["source_revision"], Value::Null);
    assert_eq!(value["agent"]["native_reports"], 1);
    assert_eq!(value["agent"]["native_reports_with_source_identity"], 0);
    assert_eq!(value["agent"]["native_reports_with_binary_identity"], 0);

    let error = build(&fixture.request(CampaignMode::Final, 1, IdentityPolicy::RequireNative))
        .expect_err("strict identity policy");
    assert!(error.to_string().contains("has no source revision"));
}

#[test]
fn legacy_report_preserves_null_harbor_usage() {
    let fixture = Fixture::new(1, 1, true);
    fixture.add_trial("example__one", None);
    fixture.remove_trial_usage("example__one");

    let report = build(&fixture.request(CampaignMode::Final, 1, IdentityPolicy::AllowLegacy))
        .expect("legacy report with null usage");
    let value = serde_json::to_value(report).expect("serialized report");

    assert_eq!(value["trials"][0]["usage"], Value::Null);
    assert_eq!(value["aggregate"]["native_requests"], 0);
    assert_eq!(value["aggregate"]["input_tokens"], 0);
    assert_eq!(value["aggregate"]["native_accepted"], 1);
}

fn write_json(path: &std::path::Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("fixture JSON"))
        .expect("write fixture JSON");
}
