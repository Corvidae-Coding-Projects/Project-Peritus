//! Strict immutable daemon configuration generated from durable product state.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use peritus_daemon::{DaemonConfig, DaemonIdentity, DaemonPaths, LocalEndpointAddress};
use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProductState, ProviderKind, WorkspaceTrust,
};

use crate::{AppLayout, LauncherError, persistence::read_exact_or_publish};

pub fn ensure_configuration(
    layout: &AppLayout,
    state: &ProductState,
) -> Result<(DaemonConfig, PathBuf), LauncherError> {
    let text = render_configuration(layout, state)?;
    let expected = DaemonConfig::parse(&text)?;
    let path = layout.daemon_config(state.generation());
    let actual = read_exact_or_publish(&path, text.as_bytes())?;
    if actual != text.as_bytes() {
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
        "version = 1\nstore_id = {:?}\n\n[paths]\nstate_root = {}\nartifact_root = {}\nevidence_root = {}\nworkspace_root = {}\nprocess_root = {}\ntransaction_root = {}\nbackup_root = {}\n\n[approval_registry]\npayload_file = {}\ngeneration = 1\n\n[human]\nactor_id = {:?}\n\n[telemetry]\nmode = \"disabled\"\n",
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
    );
    for provider in state.providers().enabled() {
        text.push_str(&render_provider(*provider, state.providers().direct_profile(*provider))?);
    }
    render_workspaces(&mut text, state)?;
    Ok(text)
}

fn render_workspaces(text: &mut String, state: &ProductState) -> Result<(), LauncherError> {
    for profile in state.workspaces().registered() {
        writeln!(
            text,
            "\n[[projects]]\nproject_id = {}\nworkspace_ids = [{}]\n",
            toml_string(profile.project_id()),
            toml_string(profile.workspace_id()),
        )
        .expect("writing to String cannot fail");
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
    direct: Option<&DirectProviderProfile>,
) -> Result<String, LauncherError> {
    let (kind, profile_id, model) = match provider {
        ProviderKind::CodexAccount => {
            ("codex-runtime", "a1000000000000000000000000000001", "gpt-5.6-sol")
        }
        ProviderKind::ClaudeAccount => {
            ("claude-runtime", "a2000000000000000000000000000002", "opus")
        }
        _ => return render_direct_provider(provider, direct),
    };
    Ok(provider_configuration(kind, profile_id, model, 200_000, 64_000, false))
}

fn render_direct_provider(
    provider: ProviderKind,
    direct: Option<&DirectProviderProfile>,
) -> Result<String, LauncherError> {
    let direct = direct.ok_or_else(|| {
        peritus_product_state::ProductStateError::InvalidPayload(
            "enabled direct provider is missing its profile".to_owned(),
        )
    })?;
    let (kind, profile_id, input, output) = direct_route(provider, direct)?;
    let mut text = format!(
        "\n[[providers]]\nkind = {}\ncredential_reference = {}\n",
        toml_string(kind),
        toml_string(direct.credential_reference())
    );
    append_optional(&mut text, "endpoint", direct.endpoint());
    append_optional(&mut text, "credential_header", direct.credential_header());
    text.push_str(&profile_block(profile_id, direct.model(), input, output, true));
    Ok(text)
}

fn direct_route(
    provider: ProviderKind,
    direct: &DirectProviderProfile,
) -> Result<(&'static str, &'static str, u64, u64), LauncherError> {
    match provider {
        ProviderKind::OpenAiApi => {
            Ok(("open-ai", "a3000000000000000000000000000003", 200_000, 64_000))
        }
        ProviderKind::AnthropicApi => {
            Ok(("anthropic", "a4000000000000000000000000000004", 200_000, 32_000))
        }
        ProviderKind::GoogleGeminiApi => {
            Ok(("google-generate-content", "a5000000000000000000000000000005", 1_000_000, 65_536))
        }
        ProviderKind::CompatibleEndpoint => match direct.compatible_protocol() {
            Some(CompatibleProtocol::Responses) => {
                Ok(("compatible-responses", "a6000000000000000000000000000006", 200_000, 32_000))
            }
            Some(CompatibleProtocol::ChatCompletions) => Ok((
                "compatible-chat-completions",
                "a6000000000000000000000000000006",
                200_000,
                32_000,
            )),
            None => Err(invalid("compatible provider is missing its wire protocol")),
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

fn provider_configuration(
    kind: &str,
    profile_id: &str,
    model: &str,
    input: u64,
    output: u64,
    streaming: bool,
) -> String {
    let mut text = format!("\n[[providers]]\nkind = {}\n", toml_string(kind));
    text.push_str(&profile_block(profile_id, model, input, output, streaming));
    text
}

fn profile_block(
    profile_id: &str,
    model: &str,
    input: u64,
    output: u64,
    streaming: bool,
) -> String {
    let capabilities = if streaming {
        "[\"parallel-tool-calls\", \"streaming\", \"tool-calls\", \"usage-detail\"]"
    } else {
        "[\"parallel-tool-calls\", \"tool-calls\", \"usage-detail\"]"
    };
    format!(
        "\n[providers.profile]\nprofile_id = {}\nrevision = 1\nmodel = {}\ncapabilities = {capabilities}\nmax_input_tokens = {input}\nmax_output_tokens = {output}\nmax_tools = 64\nmax_parallel_tool_calls = 8\nmax_inline_media_bytes = 1\n",
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

pub fn endpoint(configuration: &DaemonConfig) -> LocalEndpointAddress {
    let store = configuration.store_identity().expect("validated daemon store identity");
    let identity = DaemonIdentity::new(store);
    #[cfg(unix)]
    {
        LocalEndpointAddress::Unix(
            configuration.paths().state_root().join(format!("{}.sock", identity.endpoint_name())),
        )
    }
    #[cfg(windows)]
    {
        LocalEndpointAddress::Windows(format!(r"\\.\pipe\{}", identity.endpoint_name()))
    }
}

fn invalid(detail: &'static str) -> LauncherError {
    peritus_product_state::ProductStateError::InvalidPayload(detail.to_owned()).into()
}
