//! Hermetic constrained-runtime request, output, failure, and ownership tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use peritus_model_protocol::{FailureCategory, ModelEvent, OutputLimitEnforcement};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ProcessExit, ProcessLimits, ProcessOutput,
    ProcessRequest, ProcessTransport, ProviderCoreError,
};

use super::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use crate::test_support::{block_on, fixture, profile, runtime_profile, runtime_request};

enum Script {
    Output { success: bool, stdout: Vec<u8>, stderr: Vec<u8> },
    Error(ProviderCoreError),
}

#[derive(Clone, Debug)]
struct Capture {
    arguments: Vec<String>,
    stdin: Vec<u8>,
    environment: Vec<String>,
    current_dir: bool,
    system: Vec<u8>,
    debug: String,
}

#[derive(Default)]
struct FakeState {
    scripts: Mutex<VecDeque<Script>>,
    captures: Mutex<Vec<Capture>>,
}

struct FakeProcess(Arc<FakeState>);

impl ProcessTransport for FakeProcess {
    fn run<'a>(
        &'a self,
        request: ProcessRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ProcessOutput, ProviderCoreError>> {
        let system = argument_after(request.arguments(), "--system-prompt-file")
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default();
        self.0.captures.lock().expect("capture lock").push(Capture {
            arguments: request.arguments().to_vec(),
            stdin: request.stdin().to_vec(),
            environment: request
                .environment_removals()
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
            current_dir: request.current_dir().is_some(),
            system,
            debug: format!("{request:?}"),
        });
        let limits = request.limits();
        let script = self.0.scripts.lock().expect("script lock").pop_front();
        Box::pin(async move {
            match script.ok_or_else(|| {
                ProviderCoreError::transport("fake_process", "missing scripted response")
            })? {
                Script::Output { success, stdout, stderr } => ProcessOutput::new(
                    ProcessExit::new(success, Some(i32::from(!success))),
                    stdout,
                    stderr,
                    limits,
                ),
                Script::Error(error) => Err(error),
            }
        })
    }
}

fn output(name: &str) -> Script {
    Script::Output { success: true, stdout: fixture(name), stderr: Vec::new() }
}

fn state(scripts: Vec<Script>) -> Arc<FakeState> {
    Arc::new(FakeState { scripts: Mutex::new(scripts.into()), captures: Mutex::default() })
}

fn config(executable: ClaudeExecutable) -> ClaudeRuntimeConfig {
    ClaudeRuntimeConfig::new(executable, runtime_profile(), ProcessLimits::PRODUCTION)
        .expect("runtime config")
}

fn runtime_provider(state: Arc<FakeState>) -> ClaudeRuntimeProvider {
    let executable = ClaudeExecutable::pin(std::env::current_exe().expect("test executable"))
        .expect("pinned executable");
    ClaudeRuntimeProvider::with_transport(config(executable), Box::new(FakeProcess(state)))
}

fn events(
    provider: &ClaudeRuntimeProvider,
    request: peritus_model_protocol::ModelRequest,
    cancellation: CancellationToken,
) -> Vec<peritus_model_protocol::EventEnvelope> {
    block_on(async {
        let mut stream = provider.start(request, cancellation).await.expect("start");
        let mut events = Vec::new();
        while let Some(event) = stream.pull().await.expect("event") {
            let terminal = matches!(
                event.event(),
                ModelEvent::ResponseCompleted
                    | ModelEvent::ResponseFailed(_)
                    | ModelEvent::ResponseCancelled
            );
            events.push(event);
            if terminal {
                break;
            }
        }
        events
    })
}

#[test]
fn constrained_turn_disables_runtime_authority_and_normalizes_inert_output() {
    let fake = state(vec![output("runtime_auth_true.json"), output("runtime_tool.json")]);
    let provider = runtime_provider(Arc::clone(&fake));
    assert_eq!(provider.profile().output_limit_enforcement(), OutputLimitEnforcement::Advisory);
    let request = runtime_request(provider.profile(), true);
    let first = events(&provider, request.clone(), CancellationToken::new());
    assert!(matches!(
        first.first().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseStarted { .. })
    ));
    assert!(first.iter().any(|event| matches!(event.event(), ModelEvent::TextDelta { .. })));
    assert!(first.iter().any(|event| matches!(event.event(), ModelEvent::ToolCallStarted { name, .. } if name.as_str() == "lookup")));
    assert!(first.iter().any(|event| matches!(event.event(), ModelEvent::ToolArgumentDelta { fragment, .. } if fragment.expose() == br#"{"id":"42"}"#)));
    let usage = first
        .iter()
        .find_map(|event| match event.event() {
            ModelEvent::Usage(usage) => Some(usage.counters()),
            _ => None,
        })
        .expect("usage");
    assert_eq!(usage.input_tokens(), Some(12));
    assert_eq!(usage.output_tokens(), Some(7));
    assert!(matches!(
        first.last().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseCompleted)
    ));

    let captures = fake.captures.lock().expect("capture lock").clone();
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].arguments, ["auth", "status", "--json"]);
    let turn = &captures[1];
    for required in [
        "-p",
        "--safe-mode",
        "--tools",
        "--disallowedTools",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--system-prompt-file",
        "--json-schema",
    ] {
        assert!(turn.arguments.iter().any(|argument| argument == required), "missing {required}");
    }
    assert!(argument_pair(&turn.arguments, "--tools", ""));
    assert!(argument_pair(&turn.arguments, "--mcp-config", r#"{"mcpServers":{}}"#));
    assert!(argument_pair(&turn.arguments, "--disallowedTools", "mcp__*"));
    let schema = argument_after(&turn.arguments, "--json-schema").expect("schema argument");
    assert!(schema.contains("\"const\":\"lookup\""));
    let prompt = std::str::from_utf8(&turn.stdin).expect("prompt UTF-8");
    assert!(prompt.contains("look up 42"));
    assert!(!prompt.contains("lookup"));
    assert!(!prompt.contains("host policy"));
    let system = std::str::from_utf8(&turn.system).expect("system UTF-8");
    assert!(system.contains("sole agent harness"));
    assert!(system.contains("host policy"));
    assert!(turn.current_dir);
    assert_eq!(
        turn.environment,
        ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"]
    );
    assert!(!turn.debug.contains("look up 42"));
    assert!(!format!("{provider:?}").contains("look up 42"));

    let repeat_state = state(vec![output("runtime_auth_true.json"), output("runtime_tool.json")]);
    let repeat = events(&runtime_provider(repeat_state), request, CancellationToken::new());
    assert_eq!(first, repeat, "IDs, digests, and event order must be deterministic");
}

#[test]
fn auth_malformed_reported_and_cancelled_paths_are_explicit_terminals() {
    let cases = [
        (
            vec![output("runtime_auth_false.json")],
            Expected::Failure(FailureCategory::Authentication),
        ),
        (
            vec![output("runtime_auth_true.json"), output("runtime_malformed.json")],
            Expected::Failure(FailureCategory::MalformedPayload),
        ),
        (
            vec![output("runtime_auth_true.json"), output("runtime_incomplete.json")],
            Expected::Failure(FailureCategory::IncompleteStream),
        ),
        (
            vec![output("runtime_auth_true.json"), output("runtime_error.json")],
            Expected::Failure(FailureCategory::Provider),
        ),
        (
            vec![
                output("runtime_auth_true.json"),
                Script::Error(ProviderCoreError::cancelled("fake_process")),
            ],
            Expected::Cancelled,
        ),
    ];
    for (scripts, expected) in cases {
        let fake = state(scripts);
        let provider = runtime_provider(fake);
        let events =
            events(&provider, runtime_request(provider.profile(), false), CancellationToken::new());
        assert!(matches!(
            events.first().map(peritus_model_protocol::EventEnvelope::event),
            Some(ModelEvent::ResponseStarted { .. })
        ));
        match expected {
            Expected::Failure(category) => assert!(matches!(
                events.last().map(peritus_model_protocol::EventEnvelope::event),
                Some(ModelEvent::ResponseFailed(failure)) if failure.category() == category
            )),
            Expected::Cancelled => assert!(matches!(
                events.last().map(peritus_model_protocol::EventEnvelope::event),
                Some(ModelEvent::ResponseCancelled)
            )),
        }
    }
}

#[test]
fn direct_messages_profile_cannot_be_reused_for_the_runtime() {
    let executable = ClaudeExecutable::pin(std::env::current_exe().expect("test executable"))
        .expect("pinned executable");
    assert!(ClaudeRuntimeConfig::new(executable, profile(), ProcessLimits::PRODUCTION).is_err());
}

enum Expected {
    Failure(FailureCategory),
    Cancelled,
}

fn argument_after<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].as_str())
}

fn argument_pair(arguments: &[String], name: &str, value: &str) -> bool {
    arguments.windows(2).any(|pair| pair[0] == name && pair[1] == value)
}
