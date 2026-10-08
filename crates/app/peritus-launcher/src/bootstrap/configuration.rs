//! Strict immutable daemon configuration generated from durable product state.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use peritus_daemon::{DaemonConfig, DaemonIdentity, DaemonPaths, LocalEndpointAddress};
use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProductState, ProviderKind, WorkspaceTrust,
};

use crate::{AppLayout, LauncherError, persistence::read_exact_or_publish};
pub(super) mod account;
mod compatibility;
mod folder;
mod hosted;

/// Imports the exact models from a pre-conversation immutable configuration. This migration
/// never substitutes a newly chosen provider default or edits an old configuration generation.
pub(super) fn retain_legacy_models(
    layout: &AppLayout,
    store: &crate::persistence::ProductStateStore,
    state: &mut ProductState,
) -> Result<(), LauncherError> {
    let missing = state
        .providers()
        .enabled()
        .iter()
        .copied()
        .any(|kind| kind.is_account() && state.providers().account_model(kind).is_none());
    if !missing {
        return Ok(());
    }
    let path = layout.daemon_config(state.generation());
    let text = std::fs::read_to_string(path).map_err(|_| {
        invalid(
            "account models must be explicitly selected before publishing provider configuration",
        )
    })?;
    let prior = DaemonConfig::parse(&text)?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|_| invalid("prior model configuration is malformed"))?;
    let _ = prior;
    let routes = value
        .get("providers")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| invalid("prior provider configuration is missing"))?;
    let mut models = std::collections::BTreeMap::new();
    for kind in state.providers().enabled().iter().copied().filter(|kind| kind.is_account()) {
        let model = if let Some(model) = state.providers().account_model(kind) {
            model
        } else {
            let route =
                if kind == ProviderKind::CodexAccount { "codex-runtime" } else { "claude-runtime" };
            routes
                .iter()
                .find(|value| value.get("kind").and_then(toml::Value::as_str) == Some(route))
                .and_then(|value| value.get("profile"))
                .and_then(|value| value.get("model"))
                .and_then(toml::Value::as_str)
                .ok_or_else(|| invalid("prior selected account model is missing"))?
        };
        models.insert(kind, model.to_owned());
    }
    let selection = state.providers().clone().with_account_models(models)?;
    if state.configure_providers(selection)? {
        store.commit(state)?;
    }
    Ok(())
}

pub fn ensure_configuration(
    layout: &AppLayout,
    state: &ProductState,
) -> Result<(DaemonConfig, PathBuf), LauncherError> {
    let text = render_configuration(layout, state)?;
    let expected = DaemonConfig::parse(&text)?;
    let path = layout.daemon_config(state.generation());
    let actual = read_exact_or_publish(&path, text.as_bytes())?;
    if !compatibility::matches_current_or_legacy_timeout_configuration(&actual, &text) {
        return Err(LauncherError::PlatformPaths(format!(
            "generated daemon configuration generation {} has different content",
            state.generation()
        )));
    }
    Ok((expected, path))
}

fn render_configuration(layout: &AppLayout, state: &ProductState) -> Result<String, LauncherError> {
    let daemon_root = layout.state_root().join("daemon");
    let paths = DaemonPaths::new(
        daemon_root.clone(),
        daemon_root.join("artifacts"),
        daemon_root.join("evidence"),
        daemon_root.join("workspaces"),
        daemon_root.join("processes"),
        daemon_root.join("transactions"),
        daemon_root.join("backups"),
    )?;
    let mut text = format!(
        "version = 1\nstore_id = {:?}\n\n[paths]\nstate_root = {}\nartifact_root = {}\nevidence_root = {}\nworkspace_root = {}\nprocess_root = {}\ntransaction_root = {}\nbackup_root = {}\n\n[approval_registry]\npayload_file = {}\ngeneration = 1\n\n[human]\nactor_id = {:?}\n\n[product]\nautomatic_provider_failover = {}\n\n[telemetry]\nmode = \"disabled\"\n",
        state.identity().store_id(),
        toml_path(paths.state_root())?,
        toml_path(paths.artifact_root())?,
        toml_path(paths.evidence_root())?,
        toml_path(paths.workspace_root())?,
        toml_path(paths.process_root())?,
        toml_path(paths.transaction_root())?,
        toml_path(paths.backup_root())?,
        toml_path(&layout.approval_registry())?,
        state.identity().actor_id(),
        state.providers().automatic_failover(),
    );
    for route in state.providers().routes() {
        text.push_str(&render_provider(
            route.kind(),
            route.identity(),
            state.providers().direct_profile_by_route(route.identity()),
            state.providers().account_model(route.kind()),
            state.providers().account_executable(route.kind()),
        )?);
    }
    render_workspaces(&mut text, state)?;
    folder::render(&mut text, layout, state)?;
    Ok(text)
}

fn render_workspaces(text: &mut String, state: &ProductState) -> Result<(), LauncherError> {
    let registered = state.workspaces().registered();
    let mut projects = BTreeMap::<&str, Vec<&str>>::new();
    for profile in &registered {
        projects.entry(profile.project_id()).or_default().push(profile.workspace_id());
    }
    for (project, workspaces) in projects {
        let workspace_ids = workspaces
            .into_iter()
            .map(toml_string)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            text,
            "\n[[projects]]\nproject_id = {}\nworkspace_ids = [{workspace_ids}]\n",
            toml_string(project),
        )
        .expect("writing to String cannot fail");
    }
    for profile in registered {
        let registration = profile
            .registration_file()
            .ok_or_else(|| invalid("trusted workspace is missing its C1 registration file"))?;
        writeln!(
            text,
            "\n[[workspaces]]\nregistration_file = {}\n",
            toml_path(Path::new(registration))?,
        )
        .expect("writing to String cannot fail");
    }
    text.push_str("\n[tools]\nallow = [");
    if state
        .workspaces()
        .active()
        .is_some_and(|profile| profile.trust_level() == WorkspaceTrust::Trusted)
    {
        text.push_str(
            "\"fs.create\", \"fs.discover\", \"fs.metadata\", \"fs.patch\", \"fs.read\", \"fs.remove\", \"fs.replace\", \"fs.search\", \"fs.write\", \"git.candidate\", \"git.diff\", \"git.history\", \"git.rollback\", \"git.snapshot\", \"git.status\", \"quality.discover\", \"quality.run\", \"shell.exec\", \"shell.script\"",
        );
    }
    text.push_str("]\n");
    Ok(())
}

fn render_provider(
    provider: ProviderKind,
    route_identity: peritus_product_state::ProviderRouteIdentity,
    direct: Option<&DirectProviderProfile>,
    account_model: Option<&str>,
    account_executable: Option<&str>,
) -> Result<String, LauncherError> {
    let (kind, image_input) = match provider {
        ProviderKind::CodexAccount => ("codex-runtime", true),
        ProviderKind::ClaudeAccount => ("claude-runtime", false),
        _ => return render_direct_provider(provider, direct),
    };
    let model = account_model.ok_or_else(|| {
        invalid("account provider has no selected model; complete provider model setup")
    })?;
    let mut text = format!("\n[[providers]]\nkind = {}\n", toml_string(kind));
    // Discovery is durably reconciled before rendering. An updated native client must create a
    // new product generation rather than changing the bytes of an existing configuration.
    if let Some(executable) = account_executable {
        writeln!(text, "executable = {}", toml_string(executable))
            .expect("writing to String cannot fail");
    }
    text.push_str(&profile_block(
        &route_identity.to_string(),
        model,
        200_000,
        64_000,
        ProfileFeatures::account(image_input),
    ));
    Ok(text)
}

pub fn render_direct_provider(
    provider: ProviderKind,
    direct: Option<&DirectProviderProfile>,
) -> Result<String, LauncherError> {
    let direct = direct.ok_or_else(|| {
        peritus_product_state::ProductStateError::InvalidPayload(
            "enabled direct provider is missing its profile".to_owned(),
        )
    })?;
    if let Some(service) = provider.hosted_service() {
        return hosted::render(direct, service);
    }
    let (kind, input, output, image_input, reasoning) = direct_route(provider, direct)?;
    let mut text = format!(
        "\n[[providers]]\nkind = {}\ncredential_reference = {}\n",
        toml_string(kind),
        toml_string(direct.credential_reference())
    );
    append_optional(&mut text, "endpoint", direct.endpoint());
    append_optional(&mut text, "catalog_endpoint", direct.catalog_endpoint());
    append_optional(&mut text, "credential_header", direct.credential_header());
    text.push_str(&profile_block(
        &direct.route_identity().to_string(),
        direct.model(),
        input,
        output,
        ProfileFeatures::direct(provider, image_input, reasoning),
    ));
    Ok(text)
}

fn direct_route(
    provider: ProviderKind,
    direct: &DirectProviderProfile,
) -> Result<(&'static str, u64, u64, bool, bool), LauncherError> {
    match provider {
        ProviderKind::OpenAiApi => Ok(("open-ai", 200_000, 64_000, true, true)),
        ProviderKind::AnthropicApi => Ok(("anthropic", 200_000, 32_000, true, true)),
        ProviderKind::GoogleGeminiApi => Ok((
            "google-generate-content",
            1_000_000,
            65_536,
            true,
            true,
        )),
        ProviderKind::CompatibleEndpoint => match direct.compatible_protocol() {
            Some(CompatibleProtocol::Responses) => Ok((
                "compatible-responses",
                200_000,
                32_000,
                false,
                false,
            )),
            Some(CompatibleProtocol::ChatCompletions) => Ok((
                "compatible-chat-completions",
                200_000,
                32_000,
                false,
                false,
            )),
            _ => Err(invalid("compatible provider is missing its supported wire protocol")),
        },
        _ => Err(invalid("account provider was routed through direct configuration")),
    }
}

fn append_optional(text: &mut String, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        text.push_str(field);
        text.push_str(" = ");
        text.push_str(&toml_string(value));
        text.push('\n');
    }
}

struct ProfileFeatures {
    capabilities: Vec<&'static str>,
    inline_media_bytes: u64,
}

impl ProfileFeatures {
    fn account(image_input: bool) -> Self {
        let mut capabilities = vec![
            "parallel-tool-calls",
            "prompt-caching",
            "reasoning-controls",
            "tool-calls",
            "usage-detail",
        ];
        if image_input {
            capabilities.push("image-input");
        }
        Self::new(capabilities, image_input)
    }

    fn direct(provider: ProviderKind, image_input: bool, reasoning: bool) -> Self {
        let mut capabilities =
            vec!["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"];
        if provider != ProviderKind::CompatibleEndpoint {
            capabilities.push("prompt-caching");
        }
        if image_input {
            capabilities.push("image-input");
        }
        if reasoning {
            capabilities.push("reasoning-controls");
            capabilities.push("reasoning-summaries");
        }
        Self::new(capabilities, image_input)
    }

    fn new(mut capabilities: Vec<&'static str>, image_input: bool) -> Self {
        capabilities.sort_unstable();
        let inline_media_bytes = if image_input { 32 * 1024 * 1024 } else { 1 };
        Self { capabilities, inline_media_bytes }
    }
}

fn profile_block(
    profile_id: &str,
    model: &str,
    input: u64,
    output: u64,
    features: ProfileFeatures,
) -> String {
    let capabilities = toml::Value::Array(
        features
            .capabilities
            .into_iter()
            .map(|value| toml::Value::String(value.to_owned()))
            .collect(),
    );
    let inline_media_bytes = features.inline_media_bytes;
    format!(
        "\n[providers.profile]\nprofile_id = {}\nrevision = 1\nmodel = {}\ncapabilities = {capabilities}\nmax_input_tokens = {input}\nmax_output_tokens = {output}\nmax_tools = 64\nmax_parallel_tool_calls = 8\nmax_inline_media_bytes = {inline_media_bytes}\n",
        toml_string(profile_id),
        toml_string(model),
    )
}

fn toml_path(path: &Path) -> Result<String, LauncherError> {
    let text = path.to_str().ok_or_else(|| {
        LauncherError::PlatformPaths(format!(
            "application path is not representable in strict UTF-8 configuration: {}",
            path.display()
        ))
    })?;
    Ok(toml_string(text))
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

pub fn endpoint(configuration: &DaemonConfig) -> Result<LocalEndpointAddress, LauncherError> {
    let store = configuration.store_identity()?;
    let identity = DaemonIdentity::new(store);
    #[cfg(unix)]
    {
        let original =
            configuration.paths().state_root().join(format!("{}.sock", identity.endpoint_name()));
        peritus_local_socket::bounded_path(&original, peritus_local_socket::NATIVE_MAX_PATH_BYTES)
            .map(LocalEndpointAddress::Unix)
            .map_err(|error| LauncherError::filesystem("derive Unix endpoint", original, error))
    }
    #[cfg(windows)]
    {
        Ok(LocalEndpointAddress::Windows(format!(r"\\.\pipe\{}", identity.endpoint_name())))
    }
}

fn invalid(detail: &'static str) -> LauncherError {
    peritus_product_state::ProductStateError::InvalidPayload(detail.to_owned()).into()
}
