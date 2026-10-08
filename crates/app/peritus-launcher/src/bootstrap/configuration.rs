//! Strict immutable daemon configuration generated from durable product state.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use peritus_daemon::{DaemonConfig, DaemonIdentity, DaemonPaths, LocalEndpointAddress};
use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProductState, ProviderKind,
    ProviderModelCapability, ProviderModelFactSource, ProviderModelFacts, ProviderRouteIdentity,
    ProviderSelection, WorkspaceTrust,
};

use crate::{AppLayout, LauncherError, persistence::read_exact_or_publish};
pub(super) mod account;
mod compatibility;
mod folder;
mod hosted;

/// Imports exact model facts from a pre-fact immutable configuration. Imported values remain
/// marked as legacy generated claims; no route default is re-created or presented as discovery.
pub(super) fn retain_legacy_models(
    layout: &AppLayout,
    store: &crate::persistence::ProductStateStore,
    state: &mut ProductState,
) -> Result<(), LauncherError> {
    let missing = state
        .providers()
        .routes()
        .into_iter()
        .any(|route| {
            state.providers().model_facts(route.identity()).is_none()
                || route.kind().is_account()
                    && state.providers().account_model(route.kind()).is_none()
        });
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
    let mut direct_profiles = Vec::new();
    for direct in state.providers().direct_profiles() {
        if direct.model_facts().is_some() {
            direct_profiles.push(direct.clone());
            continue;
        }
        let route = configured_route(routes, direct.route_identity())?;
        direct_profiles.push(
            direct
                .clone()
                .with_model_facts(legacy_model_facts(route, direct.model())?)?,
        );
    }
    let mut models = BTreeMap::new();
    let mut model_facts = BTreeMap::new();
    let mut executables = BTreeMap::new();
    for kind in state.providers().enabled().iter().copied().filter(|kind| kind.is_account()) {
        let identity = state
            .providers()
            .routes()
            .into_iter()
            .find(|route| route.kind() == kind)
            .map(|route| route.identity())
            .ok_or_else(|| invalid("prior account provider route is missing"))?;
        let route = configured_route(routes, identity)?;
        let model = state
            .providers()
            .account_model(kind)
            .or_else(|| {
                route
                    .get("profile")
                    .and_then(|value| value.get("model"))
                    .and_then(toml::Value::as_str)
            })
            .ok_or_else(|| invalid("prior selected account model is missing"))?;
        let facts = match state.providers().account_model_facts(kind) {
            Some(facts) => facts.clone(),
            None => legacy_model_facts(route, model)?,
        };
        models.insert(kind, model.to_owned());
        model_facts.insert(kind, facts);
        if let Some(executable) = state.providers().account_executable(kind) {
            executables.insert(kind, executable.to_owned());
        }
    }
    let selection = ProviderSelection::with_routes_and_failover(
        state.providers().enabled().to_vec(),
        state.providers().default_route(),
        direct_profiles,
        state.providers().automatic_failover(),
    )?
    .with_account_models(models)?
    .with_account_model_facts(model_facts)?
    .with_account_executables(executables)?;
    let changed = state.configure_providers(selection)?;
    let migrated = state.migrate_legacy_storage()?;
    if changed || migrated {
        store.commit(state)?;
    }
    Ok(())
}

fn configured_route<'a>(
    routes: &'a [toml::Value],
    identity: ProviderRouteIdentity,
) -> Result<&'a toml::Value, LauncherError> {
    let identity = identity.to_string();
    routes
        .iter()
        .find(|route| {
            route
                .get("profile")
                .and_then(|profile| profile.get("profile_id"))
                .and_then(toml::Value::as_str)
                == Some(identity.as_str())
        })
        .ok_or_else(|| invalid("prior exact provider profile is missing"))
}

fn legacy_model_facts(
    route: &toml::Value,
    selected_model: &str,
) -> Result<ProviderModelFacts, LauncherError> {
    let profile = route
        .get("profile")
        .ok_or_else(|| invalid("prior provider profile is missing"))?;
    if profile.get("model").and_then(toml::Value::as_str) != Some(selected_model) {
        return Err(invalid("prior provider profile belongs to a different model"));
    }
    let capabilities = profile
        .get("capabilities")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| invalid("prior provider capabilities are missing"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .and_then(ProviderModelCapability::parse)
                .ok_or_else(|| invalid("prior provider capability cannot be retained exactly"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let u64_field = |name| {
        profile
            .get(name)
            .and_then(toml::Value::as_integer)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| invalid("prior provider capacity is missing or invalid"))
    };
    let u32_field = |name| {
        u64_field(name).and_then(|value| {
            u32::try_from(value)
                .map_err(|_| invalid("prior provider capacity is not representable"))
        })
    };
    let revision = profile
        .get("revision")
        .and_then(toml::Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| invalid("prior provider profile revision is missing or invalid"))?;
    ProviderModelFacts::new(
        ProviderModelFactSource::LegacyGenerated,
        selected_model.to_owned(),
        capabilities,
        u64_field("max_input_tokens")?,
        u64_field("max_output_tokens")?,
        u32_field("max_tools")?,
        u32_field("max_parallel_tool_calls")?,
        u64_field("max_inline_media_bytes")?,
    )
    .and_then(|facts| facts.with_revision(revision))
    .map_err(LauncherError::from)
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
    let configuration = if actual == text.as_bytes() {
        expected
    } else {
        let actual = std::str::from_utf8(&actual).map_err(|error| {
            LauncherError::PlatformPaths(format!(
                "accepted daemon configuration generation {} is not UTF-8: {error}",
                state.generation(),
            ))
        })?;
        DaemonConfig::parse(actual)?
    };
    Ok((configuration, path))
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
            state.providers().model_facts(route.identity()),
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
        .profiles()
        .into_iter()
        .any(|profile| profile.trust_level() == WorkspaceTrust::Trusted)
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
    model_facts: Option<&ProviderModelFacts>,
) -> Result<String, LauncherError> {
    let kind = match provider {
        ProviderKind::CodexAccount => "codex-runtime",
        ProviderKind::ClaudeAccount => "claude-runtime",
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
    let facts = model_facts.ok_or_else(|| {
        invalid("account model capacity is unknown; reopen provider setup to resolve it")
    })?;
    if facts.model() != model {
        return Err(invalid("account model facts belong to a different selected model"));
    }
    text.push_str(&profile_block(&route_identity.to_string(), facts, &[]));
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
    let kind = direct_route(provider, direct)?;
    let mut text = format!(
        "\n[[providers]]\nkind = {}\ncredential_reference = {}\n",
        toml_string(kind),
        toml_string(direct.credential_reference())
    );
    append_optional(&mut text, "endpoint", direct.endpoint());
    append_optional(&mut text, "catalog_endpoint", direct.catalog_endpoint());
    append_optional(&mut text, "credential_header", direct.credential_header());
    let facts = direct.model_facts().ok_or_else(|| {
        invalid("direct model capacity is unknown; reopen provider setup to resolve it")
    })?;
    text.push_str(&profile_block(
        &direct.route_identity().to_string(),
        facts,
        &[],
    ));
    Ok(text)
}

fn direct_route(
    provider: ProviderKind,
    direct: &DirectProviderProfile,
) -> Result<&'static str, LauncherError> {
    match provider {
        ProviderKind::OpenAiApi => Ok("open-ai"),
        ProviderKind::AnthropicApi => Ok("anthropic"),
        ProviderKind::GoogleGeminiApi => Ok("google-generate-content"),
        ProviderKind::CompatibleEndpoint => match direct.compatible_protocol() {
            Some(CompatibleProtocol::Responses) => Ok("compatible-responses"),
            Some(CompatibleProtocol::ChatCompletions) => Ok("compatible-chat-completions"),
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

fn profile_block(
    profile_id: &str,
    facts: &ProviderModelFacts,
    adapter_capabilities: &[ProviderModelCapability],
) -> String {
    let mut selected = facts.capabilities().to_vec();
    selected.extend_from_slice(adapter_capabilities);
    selected.sort_unstable();
    selected.dedup();
    let capabilities = toml::Value::Array(
        selected
            .into_iter()
            .map(|value| toml::Value::String(value.as_str().to_owned()))
            .collect(),
    );
    format!(
        "\n[providers.profile]\nprofile_id = {}\nrevision = {}\nmodel = {}\nfacts_version = {}\nfacts_source = {}\ncapabilities = {capabilities}\nmax_input_tokens = {}\nmax_output_tokens = {}\nmax_tools = {}\nmax_parallel_tool_calls = {}\nmax_inline_media_bytes = {}\n",
        toml_string(profile_id),
        facts.revision(),
        toml_string(facts.model()),
        facts.version(),
        toml_string(facts.source().as_str()),
        facts.max_input_tokens(),
        facts.max_output_tokens(),
        facts.max_tools(),
        facts.max_parallel_tool_calls(),
        facts.max_inline_media_bytes(),
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
