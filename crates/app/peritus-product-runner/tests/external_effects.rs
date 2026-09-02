//! Caller-authorized external-effect product-run regression.

#[path = "production_composition/support.rs"]
#[allow(dead_code, reason = "shared integration support also serves the composition fixtures")]
mod support;

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use peritus_product_runner::{
    PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope, ProductRunInput, ProductRunOutcome,
    ProductRunner, RoleProviders,
};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

use support::{
    FixedConversation, ScriptedProvider, design_response, git, list_arguments, named_tool_response,
    named_tool_response_with_id, profile, read_arguments, text_response, tool_response,
    write_arguments,
};

fn command_arguments(program: &str, args: &[&str], purpose: &str) -> Vec<u8> {
    let value = serde_json::Value::Object(
        [
            ("program".to_owned(), serde_json::Value::String(program.to_owned())),
            (
                "args".to_owned(),
                serde_json::Value::Array(
                    args.iter()
                        .map(|argument| serde_json::Value::String((*argument).to_owned()))
                        .collect(),
                ),
            ),
            ("purpose".to_owned(), serde_json::Value::String(purpose.to_owned())),
        ]
        .into_iter()
        .collect(),
    );
    serde_json::to_vec(&value).expect("command arguments")
}

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output =
        std::process::Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("git output is UTF-8")
}

#[test]
fn external_effect_command_fixture() {
    assert!(std::env::current_exe().is_ok());
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete external-effect composition fixture")]
fn authorized_external_effects_complete_without_a_synthetic_workspace_diff() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            fs::write(repository.path().join("README.md"), "# External fixture\n")
                .expect("repository file");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let executable = std::env::current_exe().expect("test executable");
            let executable = executable.to_string_lossy().into_owned();
            let writer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0xA1; 16], "external-writer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("README.md")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("README.md")),
                    named_tool_response_with_id(
                        "run_command",
                        "effect",
                        command_arguments(
                            &executable,
                            &["--exact", "external_effect_command_fixture"],
                            "external_effect",
                        ),
                    ),
                    named_tool_response_with_id(
                        "run_command",
                        "verification",
                        command_arguments(
                            &executable,
                            &["--exact", "external_effect_command_fixture"],
                            "verification",
                        ),
                    ),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"inspect the configured service","summary":"Applied and freshly verified the authorized external state."}"#,
                    ),
                ])),
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0xA2; 16], "external-reviewer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("README.md")),
                    text_response(
                        br#"{"findings":[],"summary":"The authorized effect and fresh verification command both succeeded."}"#,
                    ),
                ])),
            });
            let task = "Apply and verify the authorized external fixture state.".to_owned();
            let run_id = RunId::new([0xA3; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("external.trace"),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::AuthorizedExternalEffects,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&writer),
                        reviewer,
                        fixer: writer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("external-effect run");
            let ProductRunOutcome::Complete(output) = outcome else {
                panic!("run asked for unexpected user input");
            };

            assert!(output.changed_paths.is_empty());
            assert_eq!(output.successful_commands.len(), 2);
            assert!(output.gates.contains("Exact-target acceptance: NOT APPLICABLE"));
            assert!(output.gates.contains("External-effect evidence: READY"));
            assert!(output.summary.contains("caller-authorized external effects"));
            assert!(git_output(repository.path(), &["status", "--porcelain"]).trim().is_empty());
        });
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete mixed-delivery regression fixture")]
fn operational_request_needs_a_live_effect_even_when_supporting_files_change() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            fs::write(repository.path().join("README.md"), "# Service fixture\n")
                .expect("repository file");
            fs::write(
                repository.path().join("peritus-workspace.toml"),
                "schema_version = 1\nkind = \"artifact\"\n",
            )
            .expect("artifact workspace marker");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let executable = std::env::current_exe().expect("test executable");
            let executable = executable.to_string_lossy().into_owned();
            let developer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0xB1; 16], "mixed-delivery-developer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("README.md")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("README.md")),
                    tool_response(write_arguments(
                        "setup-service.sh",
                        "#!/bin/sh\nset -eu\nprintf '%s\\n' 'service configured'\n",
                    )),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"run setup-service.sh","summary":"Added the service setup helper."}"#,
                    ),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response(
                        "workspace_read",
                        read_arguments("setup-service.sh"),
                    ),
                    named_tool_response_with_id(
                        "run_command",
                        "effect",
                        command_arguments(
                            &executable,
                            &["--exact", "external_effect_command_fixture"],
                            "external_effect",
                        ),
                    ),
                    named_tool_response_with_id(
                        "run_command",
                        "verification",
                        command_arguments(
                            &executable,
                            &["--exact", "external_effect_command_fixture"],
                            "verification",
                        ),
                    ),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"connect to the configured service","summary":"Applied and verified the live service state."}"#,
                    ),
                ])),
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0xB2; 16], "mixed-delivery-reviewer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response(
                        "workspace_read",
                        read_arguments("setup-service.sh"),
                    ),
                    text_response(
                        br#"{"findings":[],"summary":"The supporting helper is well formed."}"#,
                    ),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response(
                        "workspace_read",
                        read_arguments("setup-service.sh"),
                    ),
                    text_response(
                        br#"{"findings":[],"summary":"The helper and live effect evidence satisfy the request."}"#,
                    ),
                ])),
            });
            let task =
                "Configure the local fixture so that I can connect to the running service."
                    .to_owned();
            let run_id = RunId::new([0xB3; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("mixed-delivery.trace"),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::AuthorizedExternalEffects,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&developer),
                        reviewer,
                        fixer: developer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("mixed-delivery run");
            let ProductRunOutcome::Complete(output) = outcome else {
                panic!("run asked for unexpected user input");
            };

            assert_eq!(output.changed_paths, [Path::new("setup-service.sh")]);
            assert!(output.successful_commands.len() >= 2);
            assert!(output.gates.contains("required by the operational request: yes"));
            assert!(output.gates.contains("External-effect evidence: READY"));
            assert!(output.summary.contains("supporting changed file"));
            assert_eq!(output.fixer_cycles, 1);
        });
}
