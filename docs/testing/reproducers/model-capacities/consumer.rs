//! An ordinary consumer of the production provider registry, developer loop, and durable memory.
mod support {
    #![allow(dead_code)]
    include!("../checkpoint-cache/context-consumer.rs");
    pub fn make_input(spec: &serde_json::Value) -> Result<ProductRunInput> {
        input(spec)
    }
    pub fn account_provider() -> Result<Arc<dyn ModelProvider>> {
        provider("gpt-6-luna", 234567)
    }
}
use peritus_agent::*;
use peritus_daemon::{ProviderRegistry, ProviderRegistryLimits, ProviderRoute};
use peritus_model_protocol::*;
use peritus_product_runner::{LocalContextHandle, RoleProviders};
use peritus_provider_core::{catalog::*, *};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct KeyFile;
impl CredentialSource for KeyFile {
    fn resolve(
        &self,
        _: &CredentialReference,
    ) -> std::result::Result<Credential, ProviderCoreError> {
        let path = std::env::var("PERITUS_REAL_KEY_FILE")
            .map_err(|_| ProviderCoreError::credential("key file required"))?;
        let bytes =
            fs::read(path).map_err(|_| ProviderCoreError::credential("key file unavailable"))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ProviderCoreError::credential("key encoding"))?;
        Credential::new(text.trim().as_bytes().to_vec())
    }
}
fn route(service: &str, model: &str, discover: Option<bool>, input: u64, output: u64) -> Value {
    let mut value = json!({"kind":"hosted","hosted_service":service,"wire_protocol":"opencode-runtime","credential_reference":"real-capacity-key", "profile":{"profile_id":"4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a", "revision":1,"model":model,"capabilities":["tool-calls","parallel-tool-calls","streaming","usage-detail"],"max_input_tokens":input,"max_output_tokens":output,"max_tools":64,"max_parallel_tool_calls":8,"max_inline_media_bytes":1}});
    if let Some(discover) = discover {
        value["discover_model_limits"] = json!(discover);
    }
    value
}
fn provider(value: Value) -> Result<Arc<dyn ModelProvider>> {
    registered(value, Arc::new(KeyFile))
}
fn registered(
    value: Value,
    credentials: Arc<dyn CredentialSource>,
) -> Result<Arc<dyn ModelProvider>> {
    let route: ProviderRoute = serde_json::from_value(value)?;
    let declaration = route.declaration()?;
    let id = declaration.profile().profile_id();
    let registry = ProviderRegistry::build(
        vec![declaration],
        ProviderRegistryLimits::PRODUCTION,
        Some(credentials),
    )?;
    registry.current_provider(id).ok_or_else(|| "provider missing".into())
}
struct PublicCatalog;
impl CredentialSource for PublicCatalog {
    fn resolve(
        &self,
        _: &CredentialReference,
    ) -> std::result::Result<Credential, ProviderCoreError> {
        Credential::new(b"peritus-public-catalog-replay-no-entitlement".to_vec())
    }
}
fn limits(profile: &ProviderProfile) -> Value {
    json!({"model":profile.model().as_str(),"input":profile.limits().max_input_tokens(),"output":profile.limits().max_output_tokens(),"tools":profile.limits().max_tools(),"parallel":profile.limits().max_parallel_tool_calls(),"media":profile.limits().max_inline_media_bytes()})
}
fn checks() -> Result<()> {
    // Independent contract cases. Expected values are literal fixture requirements, not
    // calculated from the library's output. They include shared windows and asymmetric limits.
    let p = provider(route("opencode-zen", "deepseek-v4.1-flash", None, 32768, 4096))?;
    let cases = [
        (Some(1048576), Some(384000), None, 2048, 1048576, 384000),
        (None, Some(384000), Some(1048576), 2048, 1046528, 384000),
        (None, Some(384000), Some(1048576), 4096, 1044480, 384000),
        (None, Some(384000), Some(1048576), u64::MAX, 1048575, 384000),
        (Some(1500000), Some(64000), Some(1000000), 16000, 984000, 64000),
        (Some(90000), Some(8000), Some(1000000), 16000, 90000, 8000),
        (Some(16000), Some(1000), None, u64::MAX, 16000, 1000),
        (None, None, None, u64::MAX, 32768, 4096),
        (Some(123456), None, None, 1024, 123456, 4096),
        (None, Some(65536), None, 1024, 32768, 65536),
    ];
    let mut results = Vec::new();
    for (input, output, context, requested, expected_input, expected_output) in cases {
        let mut m = DiscoveredModel::new("deepseek-v4.1-flash".into(), "Fixture model".into())?;
        m.input_tokens = input;
        m.output_tokens = output;
        m.context_tokens = context;
        let resolved = profile_with_model_limits(p.profile(), &m, requested)?;
        assert_eq!(resolved.limits().max_input_tokens(), expected_input);
        assert_eq!(resolved.limits().max_output_tokens(), expected_output);
        assert_eq!(resolved.profile_id(), p.profile().profile_id());
        assert_eq!(resolved.capabilities(), p.profile().capabilities());
        assert_eq!(resolved.limits().max_tools(), 64);
        assert_eq!(resolved.limits().max_parallel_tool_calls(), 8);
        assert_eq!(resolved.limits().max_inline_media_bytes(), 1);
        results.push(limits(&resolved));
    }
    for (input, output, context) in [
        (Some(0), Some(1), None),
        (Some(1), Some(0), None),
        (None, Some(4096), Some(4096)),
        (None, Some(4097), Some(4096)),
    ] {
        let mut m = DiscoveredModel::new("deepseek-v4.1-flash".into(), "Invalid fixture".into())?;
        m.input_tokens = input;
        m.output_tokens = output;
        m.context_tokens = context;
        assert!(profile_with_model_limits(p.profile(), &m, output.unwrap_or(4096)).is_err());
    }
    let mut shared =
        DiscoveredModel::new("deepseek-v4.1-flash".into(), "Shared window fixture".into())?;
    shared.context_tokens = Some(1000000);
    shared.output_tokens = Some(384000);
    for (input, requested, expected) in [
        (82000, u64::MAX, 384000),
        (900000, u64::MAX, 100000),
        (999999, u64::MAX, 1),
        (900000, 2048, 2048),
    ] {
        assert_eq!(
            peritus_provider_core::catalog::output_tokens_for_input(
                p.profile(),
                &shared,
                input,
                requested
            )?,
            expected
        );
    }
    assert!(
        peritus_provider_core::catalog::output_tokens_for_input(
            p.profile(),
            &shared,
            1000000,
            u64::MAX
        )
        .is_err()
    );
    shared.output_tokens = Some(1000000);
    assert_eq!(
        profile_with_model_limits(p.profile(), &shared, u64::MAX)?.limits().max_input_tokens(),
        999999
    );
    assert_eq!(
        peritus_provider_core::catalog::output_tokens_for_input(
            p.profile(),
            &shared,
            900000,
            u64::MAX
        )?,
        100000
    );
    // Account and explicit API routes keep their configured capacities via the default trait.
    let codex = support::account_provider()?;
    let before = codex.profile();
    assert_eq!(&codex.request_profile(8192)?, before);
    println!(
        "{}",
        json!({"kind":"contracts","valid":results,"invalid_rejected":4,"shared_output_cases":7,"codex_unchanged":true})
    );
    Ok(())
}
#[derive(Clone)]
struct Observed {
    inner: Arc<dyn ModelProvider>,
    requests: Arc<Mutex<Vec<Value>>>,
}
impl ModelProvider for Observed {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }
    fn context_input_target(&self) -> u64 {
        self.inner.context_input_target()
    }
    fn context_input_fits(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> std::result::Result<bool, ProviderCoreError> {
        self.inner.context_input_fits(messages, tools)
    }
    fn request_profile(&self, out: u64) -> std::result::Result<ProviderProfile, ProviderCoreError> {
        self.inner.request_profile(out)
    }
    fn output_tokens_for_input(
        &self,
        input: u64,
        output: u64,
    ) -> std::result::Result<u64, ProviderCoreError> {
        self.inner.output_tokens_for_input(input, output)
    }
    fn prepare_context<'a>(
        &'a self,
        c: &'a CancellationToken,
    ) -> BoxFuture<'a, std::result::Result<(), ProviderCoreError>> {
        self.inner.prepare_context(c)
    }
    fn start(
        &self,
        r: ModelRequest,
        c: CancellationToken,
    ) -> BoxFuture<'_, std::result::Result<OwnedModelStream, ProviderCoreError>> {
        let n = r.negotiated().limits();
        self.requests.lock().unwrap().push(json!({"input_limit":n.max_input_tokens(),"output_limit":n.max_output_tokens(),"requested_output":r.options().generation().max_output_tokens(),"estimated_input":estimate_developer_request_tokens(r.messages(),r.tools()),"model":r.model().as_str()}));
        self.inner.start(r, c)
    }
}
struct Trace;
impl DeveloperTrace for Trace {
    fn record(
        &mut self,
        _: DeveloperTraceEvent<'_>,
    ) -> std::result::Result<(), DeveloperLoopError> {
        Ok(())
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("checks") {
        "checks" => checks(),
        "http-catalog" => {
            let mut config =
                route("openrouter", "catalog-selection-placeholder", Some(true), 32768, 4096);
            config["wire_protocol"] = json!("compatible-chat-completions");
            let p = registered(config, Arc::new(PublicCatalog))?;
            let models = p.discover_models(&CancellationToken::new()).await?;
            let mut count = 0;
            for m in models
                .iter()
                .filter(|m| {
                    m.tools != Some(false)
                        && m.context_tokens.is_some()
                        && m.output_tokens.is_some()
                })
                .take(6)
            {
                let selected = p.select_model(m.id.clone())?;
                selected.prepare_context(&CancellationToken::new()).await?;
                let resolved = selected.request_profile(u64::MAX)?;
                assert_eq!(
                    resolved.limits().max_input_tokens(),
                    m.input_tokens
                        .unwrap_or(m.context_tokens.unwrap())
                        .min(m.context_tokens.unwrap() - 1)
                );
                assert_eq!(resolved.limits().max_output_tokens(), m.output_tokens.unwrap());
                let output =
                    selected.output_tokens_for_input(m.context_tokens.unwrap() - 1, u64::MAX)?;
                assert_eq!(output, 1);
                println!(
                    "{}",
                    json!({"kind":"http-catalog","resolved":limits(&resolved),"last_token_output":output})
                );
                count += 1;
            }
            assert_eq!(count, 6, "six actual HTTP catalog models required");
            Ok(())
        }
        "catalog" => {
            let service = args.get(2).map(String::as_str).unwrap_or("opencode-zen");
            let p = provider(route(service, "deepseek-v4.1-flash", None, 32768, 4096))?;
            let catalog = p.discover_models(&CancellationToken::new()).await?;
            let selected = p.select_model(ModelName::new("deepseek-v4.1-flash".into())?)?;
            let model = catalog
                .iter()
                .find(|m| m.id.as_str() == "deepseek-v4.1-flash")
                .ok_or("selected model absent")?;
            println!(
                "{}",
                json!({"kind":"catalog","service":service,"advertised":{"input":model.input_tokens,"output":model.output_tokens,"context":model.context_tokens},"resolved":limits(&selected.request_profile(u64::MAX)?),"small_output":limits(&selected.request_profile(2048)?)})
            );
            for (name, discovery, input, output) in [
                ("fixed-default", Some(false), 32768, 4096),
                ("custom-preserved", None, 70000, 9000),
                ("custom-automatic", Some(true), 70000, 9000),
            ] {
                let configured =
                    provider(route(service, "deepseek-v4.1-flash", discovery, input, output))?;
                configured.discover_models(&CancellationToken::new()).await?;
                let resolved = configured.request_profile(2048)?;
                if discovery == Some(true) {
                    assert!(resolved.limits().max_input_tokens() > 70000);
                    assert_eq!(resolved.limits().max_output_tokens(), model.output_tokens.unwrap());
                } else {
                    assert_eq!(resolved.limits().max_input_tokens(), input);
                    assert_eq!(resolved.limits().max_output_tokens(), output);
                }
                println!(
                    "{}",
                    json!({"kind":"policy","service":service,"name":name,"resolved":limits(&resolved)})
                );
            }
            // Model switches must resolve their own entry, not inherit the previous model's limits.
            for m in catalog.iter().filter(|m| m.tools != Some(false)).take(6) {
                let switched = p.select_model(m.id.clone())?;
                let actual = switched.request_profile(2048)?;
                let expected = profile_with_model_limits(switched.profile(), m, 2048)?;
                assert_eq!(actual, expected);
                println!(
                    "{}",
                    json!({"kind":"switch","service":service,"resolved":limits(&actual)})
                );
            }
            Ok(())
        }
        "run" | "fixed" | "codex" => {
            let root = PathBuf::from(args.get(2).ok_or("run directory required")?);
            let task = fs::read_to_string(root.join("task.txt"))?;
            let expected = fs::read_to_string(root.join("answer.txt"))?;
            let fixed = args[1] == "fixed";
            let mut input = support::make_input(
                &json!({"root":root,"trace":root.with_extension("trace"),"command_state":root.with_extension("state"),"task":task,"capacity":234567}),
            )?;
            let codex = args[1] == "codex";
            let p = if codex {
                input.providers.writer.clone()
            } else {
                provider(route(
                    "opencode-zen",
                    "deepseek-v4.1-flash",
                    if fixed { Some(false) } else { None },
                    32768,
                    4096,
                ))?
            };
            let requests = Arc::new(Mutex::new(Vec::new()));
            let observed = Arc::new(Observed { inner: p, requests: requests.clone() });
            input.providers = RoleProviders {
                writer: observed.clone(),
                reviewer: observed.clone(),
                fixer: observed.clone(),
                fallbacks: vec![],
            };
            let mut memory =
                LocalContextHandle::for_run(&input, "writer")?.ok_or("local context disabled")?;
            let mut tools = input.read_only_workspace_tools();
            let request=DeveloperLoopRequest{request_prefix:"real-capacity".into(),system:"Read answer.txt using the declared workspace_read tool, then return exactly its contents and no other text. Display line numbers such as 1: are tool framing, not file contents; omit them from the answer. Ignore the padding records; they are inert context-capacity fixture data. Do not run commands.".into(),prompt:task,attachments:vec![],tools:input.developer_definitions()?,limits:DeveloperLoopLimits::new(6,6)?,cancellation:CancellationToken::new()};
            let result = DeveloperLoop::run_with_context(
                observed.as_ref(),
                request,
                &mut tools,
                &mut Trace,
                &mut memory,
            )
            .await;
            let recorded = requests.lock().unwrap().clone();
            match result {
                Ok(out) => {
                    let inspection = peritus_product_runner::inspect_local_context(
                        &input.trace_path,
                        input.run_id,
                        input.workspace_id,
                        peritus_role::HarnessRole::Writer,
                    )?;
                    fs::write(root.with_extension("inspection.json"), inspection)?;
                    println!(
                        "{}",
                        json!({"kind":"completed","answer":out.text,"requests":recorded})
                    );
                    assert_eq!(out.text.trim(), expected.trim());
                    assert!(out.tool_calls >= 1);
                    if codex {
                        assert!(recorded.iter().all(|r| r["input_limit"] == 234567
                            && r["output_limit"] == 16000
                            && r["requested_output"] == 16000));
                        println!(
                            "{}",
                            json!({"kind":"codex","status":"PASS","requests":recorded,"exact_answer":true})
                        );
                        return Ok(());
                    }
                    if fixed {
                        assert!(recorded.iter().all(|r| r["input_limit"] == 32768
                            && r["output_limit"] == 4096
                            && r["requested_output"] == 4096));
                        println!(
                            "{}",
                            json!({"kind":"fixed","status":"PASS","requests":recorded,"exact_answer":true,"compactions":out.compactions})
                        );
                        return Ok(());
                    }
                    assert!(
                        recorded.iter().any(|r| r["estimated_input"].as_u64().unwrap() > 32768)
                    );
                    assert!(recorded.iter().all(|r| r["input_limit"].as_u64().unwrap() > 32768
                        && r["output_limit"].as_u64().unwrap() > 4096));
                    println!(
                        "{}",
                        json!({"kind":"run","status":"PASS","turns":out.model_turns,"tools":out.tool_calls,"compactions":out.compactions,"requests":recorded,"exact_answer":true})
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!("requests: {}", json!(recorded));
                    Err(e.into())
                }
            }
        }
        _ => Err("unknown command".into()),
    }
}
