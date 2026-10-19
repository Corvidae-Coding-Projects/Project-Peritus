//! Non-authoritative, content-free measurements of actual provider/context/tool work.

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
    estimate_developer_request_tokens,
};
use peritus_model_protocol::{CompletedToolCall, ContentBlock, Message, Role, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) fn event(trace: &Path, mut value: Value) {
    value["schema"] = json!(1);
    value["at_unix_ms"] = json!(now());
    let path = trace.with_extension("efficiency.jsonl");
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        file.write_all(&bytes)
    })();
    if let Err(error) = result {
        eprintln!("peritus: efficiency observation unavailable: {error}");
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Default, Deserialize, Serialize)]
struct State {
    started_unix_ms: u64,
    first_useful_action_ms: Option<u64>,
    read_bytes: u64,
    repeated_read_bytes: u64,
    seen: BTreeSet<String>,
}

pub(crate) struct MeasuredTools<'a> {
    base: &'a mut dyn DeveloperToolExecutor,
    trace: PathBuf,
    prefix: String,
    definitions: Vec<ToolDefinition>,
    started: Instant,
    state: State,
    elapsed_before: u64,
}

impl<'a> MeasuredTools<'a> {
    pub(crate) fn new(
        base: &'a mut dyn DeveloperToolExecutor,
        trace: &Path,
        prefix: &str,
        definitions: &[ToolDefinition],
        elapsed_before: u64,
    ) -> Self {
        let mut state: State = fs::read(trace.with_extension("efficiency-state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if state.started_unix_ms == 0 {
            state.started_unix_ms = now();
        }
        let result = Self {
            base,
            trace: trace.to_owned(),
            prefix: prefix.to_owned(),
            definitions: definitions.to_vec(),
            started: Instant::now(),
            state,
            elapsed_before,
        };
        result.save();
        event(trace, json!({"event":"invocation-start", "invocation":prefix}));
        result
    }

    fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.state)
            && let Some(parent) = self.trace.parent()
            && let Ok(mut file) = tempfile::NamedTempFile::new_in(parent)
            && file.write_all(&bytes).is_ok()
        {
            let _ = file.persist(self.trace.with_extension("efficiency-state.json"));
        }
    }
}

impl DeveloperToolExecutor for MeasuredTools<'_> {
    fn system_guidance(&mut self) -> Option<String> {
        self.base.system_guidance()
    }
    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        self.base.effect(call)
    }
    fn yields_to_host(&self) -> bool {
        self.base.yields_to_host()
    }
    fn required_tool_name(&self) -> Option<&str> {
        self.base.required_tool_name()
    }
    fn tool_calls_allowed(&self) -> bool {
        self.base.tool_calls_allowed()
    }
    fn completion_blocker(&self) -> Option<String> {
        self.base.completion_blocker()
    }
    fn continuation_blocker(&self) -> Option<String> {
        self.base.continuation_blocker()
    }
    fn take_progress_feedback(&mut self) -> Option<String> {
        self.base.take_progress_feedback()
    }
    fn observation_images(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Vec<peritus_model_protocol::MediaInput>, DeveloperLoopError> {
        self.base.observation_images(call, observation)
    }
    fn observe_model_context(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.base.observe_model_context(messages)?;
        let policy_bytes: usize = messages
            .iter()
            .filter(|message| message.role() == Role::System)
            .flat_map(Message::content)
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.expose_for_wire().len()),
                _ => None,
            })
            .sum();
        let definitions = if self.base.tool_calls_allowed() { &self.definitions[..] } else { &[] };
        event(
            &self.trace,
            json!({"event":"model-view", "invocation":self.prefix,
            "estimated_input_tokens":estimate_developer_request_tokens(messages,definitions),
            "advertised_tool_count":definitions.len(),
            "system_policy_bytes":policy_bytes, "message_count":messages.len()}),
        );
        Ok(())
    }
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let output = self.base.execute(call)?;
        let name = call.name().as_str();
        let arguments: Value =
            serde_json::from_slice(call.arguments().canonical_bytes()).unwrap_or(Value::Null);
        let value: Value =
            serde_json::from_slice(output.output.canonical_bytes()).unwrap_or(Value::Null);
        if !output.is_error
            && matches!(
                name,
                "workspace_read" | "skill_read" | "attachment_read" | "developer_evidence_read"
            )
        {
            let bytes = value
                .get("content")
                .or_else(|| value.get("bytes"))
                .and_then(Value::as_str)
                .map_or(0, str::len) as u64;
            let text = value
                .get("content")
                .or_else(|| value.get("bytes"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let identity = json!([
                name,
                arguments.get("path"),
                arguments.get("skill"),
                arguments.get("section"),
                text
            ]);
            let key = format!(
                "{:02x?}",
                peritus_codec::sha256(identity.to_string().as_bytes()).as_bytes()
            );
            let repeated = !self.state.seen.insert(key);
            self.state.read_bytes = self.state.read_bytes.saturating_add(bytes);
            if repeated {
                self.state.repeated_read_bytes =
                    self.state.repeated_read_bytes.saturating_add(bytes);
            }
            event(
                &self.trace,
                json!({"event":"inspection", "invocation":self.prefix,"tool":name,
                "read_bytes":bytes,"repeated_bytes":if repeated {bytes} else {0},
                "total_read_bytes":self.state.read_bytes,"total_repeated_read_bytes":self.state.repeated_read_bytes}),
            );
            self.save();
        }
        let action = !output.is_error
            && ((matches!(name, "workspace_write" | "workspace_patch" | "workspace_remove")
                && value.get("changed").and_then(Value::as_bool) != Some(false))
                || (arguments["purpose"].as_str() == Some("external_effect")
                    && value["success"].as_bool() == Some(true)
                    && ((name == "run_command" && value["state"].as_str() != Some("running"))
                        || (matches!(name, "browser_control" | "desktop_control")
                            && matches!(
                                arguments["action"].as_str(),
                                Some(
                                    "click"
                                        | "fill"
                                        | "press"
                                        | "select"
                                        | "check"
                                        | "upload"
                                        | "dialog"
                                        | "invoke"
                                        | "type"
                                )
                            )))));
        if action && self.state.first_useful_action_ms.is_none() {
            let elapsed = now().saturating_sub(self.state.started_unix_ms);
            self.state.first_useful_action_ms = Some(elapsed);
            event(
                &self.trace,
                json!({"event":"first-useful-action", "invocation":self.prefix,"tool":name,
                "wall_millis_since_first_invocation":elapsed,"invocation_elapsed_millis":self.started.elapsed().as_millis() as u64,"attempt_elapsed_millis":self.elapsed_before.saturating_add(self.started.elapsed().as_millis() as u64),
                "semantics":"first changed workspace file or successful completed declared external operation; attempt_elapsed_millis is monotonic; wall time includes pauses"}),
            );
            self.save();
        }
        Ok(output)
    }
}
