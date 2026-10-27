//! Disposable vendor configuration and short-lived private credential custody.

use peritus_provider_core::{
    Credential, EnvironmentName, ProcessLimits, ProcessRequest, ProviderCoreError,
};
use serde_json::json;
use std::{ffi::OsString, fs::OpenOptions, io::Write as _, path::Path};
use zeroize::Zeroizing;

pub struct Isolation {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
}

#[derive(serde::Serialize)]
struct ApiKey<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    key: &'a str,
}

impl Isolation {
    pub(super) fn new(
        service: &str,
        credential: &Credential,
        system: &str,
    ) -> Result<Self, ProviderCoreError> {
        Self::build(service, Some(credential), system)
    }

    pub(super) fn metadata() -> Result<Self, ProviderCoreError> {
        Self::build("opencode", None, "")
    }

    fn build(
        service: &str,
        credential: Option<&Credential>,
        system: &str,
    ) -> Result<Self, ProviderCoreError> {
        let directory = private_directory().map_err(|_| private_error())?;
        let root = std::fs::canonicalize(directory.path()).map_err(|_| private_error())?;
        reject_ambient_project(&root)?;
        for path in ["data/opencode", "config/opencode", "cache", "state", "home", "tmp"] {
            std::fs::create_dir_all(root.join(path)).map_err(|_| private_error())?;
        }
        // Never pass the key in argv, environment, or a public OpenCode login database.
        if let Some(credential) = credential {
            credential.expose_for_runtime(|bytes| {
                let key = std::str::from_utf8(bytes).map_err(|_| {
                    ProviderCoreError::credential("OpenCode requires a UTF-8 API key")
                })?;
                let auth =
                    std::collections::BTreeMap::from([(service, ApiKey { kind: "api", key })]);
                let auth = Zeroizing::new(serde_json::to_vec(&auth).map_err(|_| private_error())?);
                write_private(&root.join("data/opencode/auth.json"), &auth)
            })?;
        }
        let config = json!({
            "enabled_providers":[service], "autoupdate":false, "share":"disabled",
            "plugin":[], "mcp":{}, "lsp":false, "formatter":false,
            "compaction":{"auto":false,"prune":false}, "permission":{"*":"deny"},
            "agent":{"peritus-router":{"mode":"primary", "prompt":system,
                // The vendor injects its "tools unavailable" final-step prompt when the
                // current step >= this value. Keep the first (only expected) model turn
                // below that boundary; native tools remain disabled and denied separately.
                "permission":{"*":"deny"}, "tools":{"*":false}, "steps":2}}
        });
        write_private(
            &root.join("config/opencode/opencode.json"),
            &serde_json::to_vec(&config).map_err(|_| private_error())?,
        )?;
        Ok(Self { _directory: directory, root })
    }

    pub(super) fn process(
        &self,
        executable: &super::OpenCodeExecutable,
        args: Vec<String>,
        input: Vec<u8>,
        limits: ProcessLimits,
    ) -> Result<ProcessRequest, ProviderCoreError> {
        let root = &self.root;
        let mut removals = Vec::new();
        for (name, _) in std::env::vars_os() {
            let name = name.to_string_lossy().into_owned();
            let upper = name.to_ascii_uppercase();
            if [
                "OPENCODE_",
                "OPENAI_",
                "ANTHROPIC_",
                "CLAUDE_",
                "CODEX_",
                "GOOGLE_",
                "GEMINI_",
                "GIT_",
            ]
            .iter()
            .any(|prefix| upper.starts_with(prefix))
            {
                removals.push(EnvironmentName::new(name)?);
            }
        }
        let mut values = Vec::new();
        for (name, path) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
            ("OPENCODE_CONFIG", "config/opencode/opencode.json"),
            // v1.19.0 scans home/.opencode despite the project-config flag. Use the
            // vendor's own home override; never alter HOME or the host environment.
            ("OPENCODE_TEST_HOME", "home"),
            ("TMPDIR", "tmp"),
            ("TMP", "tmp"),
            ("TEMP", "tmp"),
            // OpenCode's CLI uses PWD before process.cwd(); both must point at private storage.
            // Core Config skips ancestor discovery when location == global config.
            ("PWD", "config/opencode"),
        ] {
            values.push((EnvironmentName::new(name.to_owned())?, root.join(path).into_os_string()));
        }
        for name in [
            "OPENCODE_DISABLE_PROJECT_CONFIG",
            "OPENCODE_DISABLE_AUTOCOMPACT",
            "OPENCODE_DISABLE_PRUNE",
            "OPENCODE_DISABLE_CLAUDE_CODE",
            "OPENCODE_DISABLE_DEFAULT_PLUGINS",
            "OPENCODE_DISABLE_EXTERNAL_SKILLS",
        ] {
            values.push((EnvironmentName::new(name.to_owned())?, OsString::from("1")));
        }
        ProcessRequest::new(
            executable.0.clone(),
            args,
            input,
            Some(root.join("config/opencode")),
            removals,
            limits,
        )?
        .with_environment_overrides(values)
    }
}

fn private_directory() -> std::io::Result<tempfile::TempDir> {
    // OpenCode writes its own SQLite database and cache. The shared temporary filesystem can
    // exhaust a user's quota even when it reports free space. Prefer the private session runtime,
    // as the official-account adapter does; retain the ambient-project check before credentials.
    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        && path.is_absolute()
        && std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        && reject_ambient_project(&path).is_ok()
        && let Ok(directory) = tempfile::Builder::new().prefix("peritus-opencode-").tempdir_in(path)
    {
        return Ok(directory);
    }
    tempfile::Builder::new().prefix("peritus-opencode-").tempdir()
}

fn reject_ambient_project(root: &Path) -> Result<(), ProviderCoreError> {
    // Defense in depth for the core loader which bypasses CLI pure/project flags.
    // Reject unsafe TMPDIR before writing a key or launching any vendor code.
    for ancestor in root.ancestors() {
        for name in
            [".git", ".opencode", "opencode.json", "opencode.jsonc", "AGENTS.md", "CONTEXT.md"]
        {
            match std::fs::symlink_metadata(ancestor.join(name)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    return Err(ProviderCoreError::configuration(
                        "opencode_runtime_isolation",
                        "temporary storage is inside an ambient project/configuration; set TMPDIR to a clean system temporary directory and retry",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), ProviderCoreError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| private_error())?;
    file.write_all(bytes).map_err(|_| private_error())?;
    file.flush().map_err(|_| private_error())
}

const fn private_error() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "opencode_runtime_storage",
        "private disposable OpenCode state could not be created",
    )
}
